//! Auto command - Autonomous mode for watching and auto-committing.

use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Local, NaiveDateTime, TimeZone};
use colored::Colorize;
use dialoguer::{Input, Select};
use rand::Rng;
use tokio::select;

use crate::config::Config;
use crate::core::ai::AiClient;
use crate::core::git;
use crate::core::lock::LockGuard;
use crate::core::watcher::{FileWatcher, WatchEvent};

/// CLI options for auto mode
pub struct AutoModeOptions {
    pub watch: bool,
    pub interval: u64,
    pub merge: bool,
    pub target: String,
    pub max_commits: usize,
    pub dry_run: bool,
    pub prompt: bool,
    pub defer: bool,
    pub spread: Option<String>,
    pub start: Option<String>,
}

/// Internal options for auto mode
struct AutoOptions {
    interval: u64,
    max_commits: usize,
    dry_run: bool,
    rewrite_history: bool,
    squash_threshold: usize,
    prompt: bool,
    defer: bool,
    spread: Option<String>,
    start: Option<String>,
}

/// A deferred commit waiting to be created
#[derive(Clone)]
struct DeferredCommit {
    message: String,
    diff: String,
    files: Vec<String>,
    timestamp: Option<DateTime<Local>>,
}

/// Batch of commits accumulated during --prompt mode
struct CommitBatch {
    commits: Vec<DeferredCommit>,
}

impl CommitBatch {
    fn new() -> Self {
        Self { commits: Vec::new() }
    }

    fn add(&mut self, commit: DeferredCommit) {
        self.commits.push(commit);
    }

    fn len(&self) -> usize {
        self.commits.len()
    }

    fn is_empty(&self) -> bool {
        self.commits.is_empty()
    }
}

/// Parse a duration string like "2h", "30m", "1d" into seconds
fn parse_duration(s: &str) -> Result<i64> {
    let s = s.trim().to_lowercase();
    let (num_str, unit) = if s.ends_with('d') {
        (&s[..s.len()-1], "d")
    } else if s.ends_with('h') {
        (&s[..s.len()-1], "h")
    } else if s.ends_with('m') {
        (&s[..s.len()-1], "m")
    } else if s.ends_with('s') {
        (&s[..s.len()-1], "s")
    } else {
        (s.as_str(), "h")
    };

    let num: i64 = num_str.parse()
        .context(format!("Invalid duration number: {}", num_str))?;

    let seconds = match unit {
        "d" => num * 86400,
        "h" => num * 3600,
        "m" => num * 60,
        "s" => num,
        _ => num * 3600,
    };

    Ok(seconds)
}

/// Parse a datetime string like "2025-12-25 09:00" into a DateTime
fn parse_start_time(s: &str) -> Result<DateTime<Local>> {
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M") {
        return Local.from_local_datetime(&naive).single()
            .context("Invalid local datetime");
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Local.from_local_datetime(&naive).single()
            .context("Invalid local datetime");
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let naive = date.and_hms_opt(9, 0, 0).context("Invalid time")?;
        return Local.from_local_datetime(&naive).single()
            .context("Invalid local datetime");
    }

    anyhow::bail!("Invalid datetime format: {}. Use YYYY-MM-DD HH:MM", s)
}

/// Generate realistic timestamps for commits spread over a duration
fn generate_spread_timestamps(
    count: usize,
    start: DateTime<Local>,
    total_duration_secs: i64,
) -> Vec<DateTime<Local>> {
    if count == 0 {
        return vec![];
    }
    if count == 1 {
        return vec![start];
    }

    let mut rng = rand::thread_rng();
    let mut timestamps = Vec::with_capacity(count);

    let base_interval = total_duration_secs / (count as i64);

    let mut current = start;
    for i in 0..count {
        timestamps.push(current);

        if i < count - 1 {
            // Add variance: 50% to 150% of base interval
            let variance = rng.gen_range(0.5..1.5);
            let interval = (base_interval as f64 * variance) as i64;

            // Add random seconds for human-like timestamps
            let extra_secs = rng.gen_range(0..60);

            current += Duration::seconds(interval.max(60) + extra_secs);
        }
    }

    // Scale back if overshot
    if let Some(last) = timestamps.last() {
        let actual_duration = (*last - start).num_seconds();
        if actual_duration > total_duration_secs {
            let scale = total_duration_secs as f64 / actual_duration as f64;
            timestamps = timestamps.iter().enumerate().map(|(i, _)| {
                if i == 0 {
                    start
                } else {
                    let offset = (timestamps[i] - start).num_seconds();
                    let scaled_offset = (offset as f64 * scale) as i64;
                    start + Duration::seconds(scaled_offset)
                }
            }).collect();
        }
    }

    timestamps
}

/// Default spread duration (2-4 hours)
fn default_spread_duration() -> i64 {
    let mut rng = rand::thread_rng();
    rng.gen_range(2..=4) * 3600
}

/// Run the auto command
pub async fn run(config: &Config, cli_options: AutoModeOptions) -> Result<()> {
    println!("{}", "gitBahn - Auto Mode".bold().cyan());
    println!();

    if cli_options.merge {
        println!("{} Auto-merge to '{}' is not yet implemented. Ignoring --merge flag.",
            "Warning:".yellow(), cli_options.target);
        println!();
    }

    // Validate flag combinations
    if cli_options.defer && !cli_options.watch {
        anyhow::bail!("--defer requires --watch mode");
    }

    if cli_options.prompt && cli_options.defer {
        anyhow::bail!("--prompt and --defer cannot be used together. Choose one mode.");
    }

    let api_key = config.anthropic_api_key()
        .context("ANTHROPIC_API_KEY not set")?;

    let ai = AiClient::new(api_key.to_string(), Some(config.ai.model.clone()));

    let options = AutoOptions {
        interval: cli_options.interval,
        max_commits: cli_options.max_commits,
        dry_run: cli_options.dry_run,
        rewrite_history: config.auto.rewrite_history,
        squash_threshold: config.auto.squash_threshold,
        prompt: cli_options.prompt,
        defer: cli_options.defer,
        spread: cli_options.spread,
        start: cli_options.start,
    };

    if cli_options.watch {
        let repo = git::open_repo(None)?;
        let repo_root = git::repo_root(&repo)?;
        let _lock = LockGuard::acquire(repo_root)?;
        drop(repo);

        if options.defer {
            run_defer_watch_mode(&ai, &options).await
        } else if options.prompt {
            run_prompt_watch_mode(&ai, &options).await
        } else {
            run_watch_mode(&ai, &options).await
        }
    } else {
        run_single(&ai, options.dry_run).await
    }
}

async fn run_single(ai: &AiClient, dry_run: bool) -> Result<()> {
    let repo = git::open_repo(None)?;

    if !git::has_uncommitted_changes(&repo)? {
        println!("{}", "No changes to commit.".dimmed());
        return Ok(());
    }

    std::process::Command::new("git")
        .args(["add", "-A"])
        .output()
        .context("Failed to stage changes")?;

    let changes = git::get_staged_changes(&repo)?;

    if changes.is_empty() {
        println!("{}", "No staged changes.".dimmed());
        return Ok(());
    }

    println!("Changes: {} (+{}, -{})",
        changes.summary(),
        changes.stats.insertions.to_string().green(),
        changes.stats.deletions.to_string().red()
    );

    let message = ai.generate_commit_message(&changes.diff, None, None).await?;

    if dry_run {
        println!("{}", "[DRY RUN]".yellow().bold());
        println!("Would commit with message:");
        println!("  {}", message);
    } else {
        let oid = git::create_commit(&repo, &message, false)?;
        println!("{} Committed: {}",
            "✓".green().bold(),
            oid.to_string()[..7].cyan()
        );
        println!("  {}", message.lines().next().unwrap_or(""));
    }

    Ok(())
}

/// Interactive prompt mode - ask user before each commit
async fn run_prompt_watch_mode(ai: &AiClient, options: &AutoOptions) -> Result<()> {
    let repo = git::open_repo(None)?;
    let repo_root = git::repo_root(&repo)?;

    println!("{}", "Interactive Mode".bold().magenta());
    println!("Watching for changes - you'll be prompted before each commit");
    println!("Press Ctrl+C to stop\n");

    let watcher = FileWatcher::new(500);
    let rx = watcher.watch(PathBuf::from(repo_root))?;

    let mut commit_count = 0;
    let mut batch = CommitBatch::new();
    let mut session_messages: Vec<String> = Vec::new(); // Track all messages in session
    let mut shutdown = false;

    while !shutdown && commit_count < options.max_commits {
        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(WatchEvent::FilesChanged(paths)) => {
                println!("\n{} {} file(s) changed",
                    "→".cyan().bold(),
                    paths.len()
                );

                // Stage and get changes
                std::process::Command::new("git")
                    .args(["add", "-A"])
                    .output()
                    .context("Failed to stage changes")?;

                let repo = git::open_repo(None)?;
                let changes = git::get_staged_changes(&repo)?;

                if changes.is_empty() {
                    continue;
                }

                println!("  {} (+{}, -{})",
                    changes.summary(),
                    changes.stats.insertions.to_string().green(),
                    changes.stats.deletions.to_string().red()
                );

                // Build context from previous session messages
                let session_context = if session_messages.is_empty() {
                    None
                } else {
                    Some(format!(
                        "Previous commits in this session (DO NOT repeat these - write something different):\n{}",
                        session_messages.iter()
                            .enumerate()
                            .map(|(i, m)| format!("  {}. {}", i + 1, m))
                            .collect::<Vec<_>>()
                            .join("\n")
                    ))
                };

                // Generate commit message with context
                let message = ai.generate_commit_message(&changes.diff, session_context.as_deref(), None).await?;
                println!("  Suggested: {}", message.lines().next().unwrap_or("").cyan());

                // Prompt user
                let choices = vec![
                    "Commit now (current time)",
                    "Commit with backdated time",
                    "Add to batch (commit later)",
                    "Skip",
                ];

                let selection = Select::new()
                    .with_prompt("What would you like to do?")
                    .items(&choices)
                    .default(0)
                    .interact()?;

                match selection {
                    0 => {
                        // Commit now with current time
                        if options.dry_run {
                            println!("{} Would commit: {}", "[DRY RUN]".yellow(), message.lines().next().unwrap_or(""));
                        } else {
                            let oid = git::create_commit(&repo, &message, false)?;
                            commit_count += 1;
                            session_messages.push(message.clone());
                            println!("{} Committed: {} - {}",
                                "✓".green().bold(),
                                oid.to_string()[..7].cyan(),
                                message.lines().next().unwrap_or("")
                            );
                        }
                    }
                    1 => {
                        // Commit with backdated time
                        let time_str: String = Input::new()
                            .with_prompt("Enter time (YYYY-MM-DD HH:MM or relative like '2h ago')")
                            .interact_text()?;

                        let timestamp = parse_time_input(&time_str)?;

                        if options.dry_run {
                            println!("{} Would commit at {}: {}",
                                "[DRY RUN]".yellow(),
                                timestamp.format("%Y-%m-%d %H:%M:%S"),
                                message.lines().next().unwrap_or("")
                            );
                        } else {
                            let oid = git::create_commit_at(&repo, &message, false, Some(timestamp))?;
                            commit_count += 1;
                            session_messages.push(message.clone());
                            println!("{} Committed at {}: {} - {}",
                                "✓".green().bold(),
                                timestamp.format("%H:%M:%S").to_string().dimmed(),
                                oid.to_string()[..7].cyan(),
                                message.lines().next().unwrap_or("")
                            );
                        }
                    }
                    2 => {
                        // Add to batch
                        let deferred = DeferredCommit {
                            message: message.clone(),
                            diff: changes.diff.clone(),
                            files: changes.all_files().iter().map(|s| s.to_string()).collect(),
                            timestamp: None,
                        };
                        batch.add(deferred);
                        session_messages.push(message); // Track batched messages too
                        println!("{} Added to batch ({} pending)",
                            "→".blue(),
                            batch.len()
                        );

                        // Unstage changes so they're not included in next commit
                        git::reset_index(&repo)?;
                    }
                    _ => {
                        // Skip
                        println!("{}", "Skipped.".dimmed());
                        git::reset_index(&repo)?;
                    }
                }
            }
            Ok(WatchEvent::Error(e)) => {
                eprintln!("{} Watcher error: {}", "Warning:".yellow(), e);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                select! {
                    biased;
                    _ = tokio::signal::ctrl_c() => {
                        println!("\n{}", "Received Ctrl+C...".yellow());
                        shutdown = true;
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(1)) => {}
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                eprintln!("{}", "Watcher disconnected".red());
                break;
            }
        }
    }

    // Handle batched commits on exit
    if !batch.is_empty() {
        println!("\n{} {} commits in batch", "→".cyan().bold(), batch.len());

        let choices = vec![
            "Commit all now (current time)",
            "Commit with spread timestamps",
            "Discard batch",
        ];

        let selection = Select::new()
            .with_prompt("How to handle batched commits?")
            .items(&choices)
            .default(1)
            .interact()?;

        match selection {
            0 => {
                // Commit all now
                for deferred in &batch.commits {
                    stage_files_for_deferred(&deferred)?;
                    let repo = git::open_repo(None)?;
                    if options.dry_run {
                        println!("{} Would commit: {}", "[DRY RUN]".yellow(), deferred.message.lines().next().unwrap_or(""));
                    } else {
                        let oid = git::create_commit(&repo, &deferred.message, false)?;
                        commit_count += 1;
                        println!("{} {} - {}",
                            "✓".green(),
                            oid.to_string()[..7].cyan(),
                            deferred.message.lines().next().unwrap_or("")
                        );
                    }
                }
            }
            1 => {
                // Spread timestamps
                let spread_duration = if let Some(ref s) = options.spread {
                    parse_duration(s)?
                } else {
                    let input: String = Input::new()
                        .with_prompt("Spread over duration (e.g., 2h, 4h, 1d)")
                        .default("2h".to_string())
                        .interact_text()?;
                    parse_duration(&input)?
                };

                let start_time = if let Some(ref s) = options.start {
                    parse_start_time(s)?
                } else {
                    let input: String = Input::new()
                        .with_prompt("Start time (YYYY-MM-DD HH:MM or 'now')")
                        .default("now".to_string())
                        .interact_text()?;
                    if input.trim().to_lowercase() == "now" {
                        Local::now()
                    } else {
                        parse_start_time(&input)?
                    }
                };

                let timestamps = generate_spread_timestamps(batch.len(), start_time, spread_duration);

                println!("\n{}", "Creating commits with spread timestamps...".bold());

                for (i, deferred) in batch.commits.iter().enumerate() {
                    stage_files_for_deferred(&deferred)?;
                    let repo = git::open_repo(None)?;
                    let ts = timestamps.get(i).copied();

                    if options.dry_run {
                        println!("{} Would commit at {}: {}",
                            "[DRY RUN]".yellow(),
                            ts.map(|t| t.format("%H:%M:%S").to_string()).unwrap_or_default(),
                            deferred.message.lines().next().unwrap_or("")
                        );
                    } else {
                        let oid = git::create_commit_at(&repo, &deferred.message, false, ts)?;
                        commit_count += 1;
                        println!("{} {} @ {} - {}",
                            "✓".green(),
                            oid.to_string()[..7].cyan(),
                            ts.map(|t| t.format("%H:%M:%S").to_string()).unwrap_or_default().dimmed(),
                            deferred.message.lines().next().unwrap_or("")
                        );
                    }
                }
            }
            _ => {
                println!("{}", "Batch discarded.".dimmed());
            }
        }
    }

    println!("\n{} Auto mode stopped. {} commits made.",
        "✓".green(),
        commit_count.to_string().cyan()
    );

    Ok(())
}

/// Deferred mode - collect all commits during session, spread on exit
async fn run_defer_watch_mode(ai: &AiClient, options: &AutoOptions) -> Result<()> {
    let repo = git::open_repo(None)?;
    let repo_root = git::repo_root(&repo)?;

    println!("{}", "Deferred Mode".bold().magenta());
    println!("Collecting commits during session - will spread timestamps on exit");
    if let Some(ref spread) = options.spread {
        println!("Spread duration: {}", spread.cyan());
    }
    println!("Press Ctrl+C to finalize\n");

    let watcher = FileWatcher::new(500);
    let rx = watcher.watch(PathBuf::from(repo_root))?;

    let mut deferred_commits: Vec<DeferredCommit> = Vec::new();
    let mut shutdown = false;

    while !shutdown && deferred_commits.len() < options.max_commits {
        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(WatchEvent::FilesChanged(paths)) => {
                println!("{} {} file(s) changed",
                    "→".dimmed(),
                    paths.len()
                );

                // Stage and get changes
                std::process::Command::new("git")
                    .args(["add", "-A"])
                    .output()
                    .context("Failed to stage changes")?;

                let repo = git::open_repo(None)?;
                let changes = git::get_staged_changes(&repo)?;

                if changes.is_empty() {
                    continue;
                }

                // Build context from previous session messages to avoid repetition
                let session_context = if deferred_commits.is_empty() {
                    None
                } else {
                    let prev_messages: Vec<String> = deferred_commits.iter()
                        .map(|d| d.message.clone())
                        .collect();
                    Some(format!(
                        "Previous commits in this session (DO NOT repeat these - write something different):\n{}",
                        prev_messages.iter()
                            .enumerate()
                            .map(|(i, m)| format!("  {}. {}", i + 1, m))
                            .collect::<Vec<_>>()
                            .join("\n")
                    ))
                };

                // Generate commit message with context
                let message = ai.generate_commit_message(
                    &changes.diff,
                    session_context.as_deref(),
                    None
                ).await?;

                let deferred = DeferredCommit {
                    message: message.clone(),
                    diff: changes.diff.clone(),
                    files: changes.all_files().iter().map(|s| s.to_string()).collect(),
                    timestamp: None,
                };

                deferred_commits.push(deferred);

                println!("{} Queued #{}: {}",
                    "◆".blue(),
                    deferred_commits.len(),
                    message.lines().next().unwrap_or("")
                );

                // Reset staging so next changes are fresh
                git::reset_index(&repo)?;
            }
            Ok(WatchEvent::Error(e)) => {
                eprintln!("{} Watcher error: {}", "Warning:".yellow(), e);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                select! {
                    biased;
                    _ = tokio::signal::ctrl_c() => {
                        println!("\n{}", "Finalizing session...".yellow());
                        shutdown = true;
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(1)) => {}
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                eprintln!("{}", "Watcher disconnected".red());
                break;
            }
        }
    }

    // Create all commits with spread timestamps
    if deferred_commits.is_empty() {
        println!("\n{}", "No commits to create.".dimmed());
        return Ok(());
    }

    println!("\n{} {} commits queued", "→".cyan().bold(), deferred_commits.len());

    // Get spread parameters
    let spread_duration = if let Some(ref s) = options.spread {
        parse_duration(s)?
    } else {
        default_spread_duration()
    };

    let start_time = if let Some(ref s) = options.start {
        parse_start_time(s)?
    } else {
        // Default: start from now minus spread_duration (so last commit is ~now)
        Local::now() - Duration::seconds(spread_duration)
    };

    let timestamps = generate_spread_timestamps(deferred_commits.len(), start_time, spread_duration);

    println!("\nSpread: {} to {}",
        timestamps.first().map(|t| t.format("%b %d %H:%M").to_string()).unwrap_or_default().cyan(),
        timestamps.last().map(|t| t.format("%b %d %H:%M").to_string()).unwrap_or_default().cyan()
    );

    // Ask for confirmation
    let choices = vec![
        "Create all commits with these timestamps",
        "Adjust spread settings",
        "Cancel (discard all)",
    ];

    let selection = Select::new()
        .with_prompt("Proceed?")
        .items(&choices)
        .default(0)
        .interact()?;

    match selection {
        0 => {
            // Create commits
            println!("\n{}", "Creating commits...".bold());

            // First, stage ALL changes that were tracked
            std::process::Command::new("git")
                .args(["add", "-A"])
                .output()
                .context("Failed to stage changes")?;

            let mut commit_count = 0;
            for (i, deferred) in deferred_commits.iter().enumerate() {
                let ts = timestamps.get(i).copied();
                let repo = git::open_repo(None)?;

                if options.dry_run {
                    println!("{} Would commit at {}: {}",
                        "[DRY RUN]".yellow(),
                        ts.map(|t| t.format("%H:%M:%S").to_string()).unwrap_or_default(),
                        deferred.message.lines().next().unwrap_or("")
                    );
                } else {
                    // For deferred mode, we stage everything once and create commits
                    // This is simplified - in real use, we'd need smarter file tracking
                    let oid = git::create_commit_at(&repo, &deferred.message, false, ts)?;
                    commit_count += 1;
                    println!("{} {} @ {} - {}",
                        "✓".green(),
                        oid.to_string()[..7].cyan(),
                        ts.map(|t| t.format("%H:%M:%S").to_string()).unwrap_or_default().dimmed(),
                        deferred.message.lines().next().unwrap_or("")
                    );
                }
            }

            println!("\n{} Created {} commits with spread timestamps.",
                "✓".green().bold(),
                commit_count.to_string().cyan()
            );
        }
        1 => {
            // Adjust settings (simplified - just re-prompt)
            let input: String = Input::new()
                .with_prompt("Enter new spread duration (e.g., 4h)")
                .interact_text()?;
            let new_duration = parse_duration(&input)?;

            let input: String = Input::new()
                .with_prompt("Enter start time (YYYY-MM-DD HH:MM)")
                .interact_text()?;
            let new_start = parse_start_time(&input)?;

            let new_timestamps = generate_spread_timestamps(deferred_commits.len(), new_start, new_duration);

            println!("\n{}", "Creating commits with adjusted timestamps...".bold());

            std::process::Command::new("git")
                .args(["add", "-A"])
                .output()
                .context("Failed to stage changes")?;

            let mut commit_count = 0;
            for (i, deferred) in deferred_commits.iter().enumerate() {
                let ts = new_timestamps.get(i).copied();
                let repo = git::open_repo(None)?;

                if !options.dry_run {
                    let oid = git::create_commit_at(&repo, &deferred.message, false, ts)?;
                    commit_count += 1;
                    println!("{} {} @ {} - {}",
                        "✓".green(),
                        oid.to_string()[..7].cyan(),
                        ts.map(|t| t.format("%H:%M:%S").to_string()).unwrap_or_default().dimmed(),
                        deferred.message.lines().next().unwrap_or("")
                    );
                }
            }

            println!("\n{} Created {} commits.",
                "✓".green().bold(),
                commit_count.to_string().cyan()
            );
        }
        _ => {
            println!("{}", "Cancelled. Changes remain unstaged.".yellow());
        }
    }

    Ok(())
}

/// Parse relative or absolute time input
fn parse_time_input(input: &str) -> Result<DateTime<Local>> {
    let input = input.trim().to_lowercase();

    // Handle relative times like "2h ago", "30m ago"
    if input.ends_with(" ago") {
        let duration_part = &input[..input.len() - 4];
        let secs = parse_duration(duration_part)?;
        return Ok(Local::now() - Duration::seconds(secs));
    }

    // Handle "now"
    if input == "now" {
        return Ok(Local::now());
    }

    // Try absolute time
    parse_start_time(&input)
}

/// Stage files for a deferred commit (best effort)
fn stage_files_for_deferred(deferred: &DeferredCommit) -> Result<()> {
    for file in &deferred.files {
        let _ = std::process::Command::new("git")
            .args(["add", file])
            .output();
    }
    Ok(())
}

// ============= Original watch modes (unchanged) =============

async fn run_watch_mode(ai: &AiClient, options: &AutoOptions) -> Result<()> {
    if options.interval == 0 {
        run_event_watch_mode(ai, options).await
    } else {
        run_polling_watch_mode(ai, options).await
    }
}

async fn run_event_watch_mode(ai: &AiClient, options: &AutoOptions) -> Result<()> {
    let repo = git::open_repo(None)?;
    let repo_root = git::repo_root(&repo)?;

    println!("Watching for file changes (event-based, max {} commits)", options.max_commits);
    if options.rewrite_history {
        println!("History rewriting enabled (squash after {} commits)", options.squash_threshold);
    }
    println!("Press Ctrl+C to stop\n");

    let watcher = FileWatcher::new(500);
    let rx = watcher.watch(PathBuf::from(repo_root))?;

    let mut commit_count = 0;
    let mut commits_since_squash = 0;
    let mut shutdown = false;

    while !shutdown && commit_count < options.max_commits {
        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(WatchEvent::FilesChanged(paths)) => {
                println!("{} {} file(s) changed",
                    "→".dimmed(),
                    paths.len()
                );
                if let Err(e) = check_and_commit(ai, options.dry_run, &mut commit_count).await {
                    eprintln!("{} {}", "Error:".red(), e);
                } else {
                    commits_since_squash += 1;

                    if options.rewrite_history && commits_since_squash >= options.squash_threshold {
                        if let Err(e) = maybe_squash_commits(ai, options.squash_threshold, options.dry_run).await {
                            eprintln!("{} Squash failed: {}", "Warning:".yellow(), e);
                        } else {
                            commits_since_squash = 0;
                        }
                    }
                }
            }
            Ok(WatchEvent::Error(e)) => {
                eprintln!("{} Watcher error: {}", "Warning:".yellow(), e);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                select! {
                    biased;
                    _ = tokio::signal::ctrl_c() => {
                        println!("\n{}", "Received Ctrl+C, shutting down gracefully...".yellow());
                        shutdown = true;
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(1)) => {}
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                eprintln!("{}", "Watcher disconnected".red());
                break;
            }
        }
    }

    if commit_count >= options.max_commits {
        println!("{}", "Max commits reached. Stopping.".yellow());
    }

    println!("{} Auto mode stopped. {} commits made.",
        "✓".green(),
        commit_count.to_string().cyan()
    );

    Ok(())
}

async fn run_polling_watch_mode(ai: &AiClient, options: &AutoOptions) -> Result<()> {
    println!("Watching for changes every {}s (max {} commits)", options.interval, options.max_commits);
    if options.rewrite_history {
        println!("History rewriting enabled (squash after {} commits)", options.squash_threshold);
    }
    println!("Press Ctrl+C to stop\n");

    let mut commit_count = 0;
    let mut commits_since_squash = 0;

    loop {
        if commit_count >= options.max_commits {
            println!("{}", "Max commits reached. Stopping.".yellow());
            break;
        }

        let old_count = commit_count;
        let should_continue = select! {
            result = check_and_commit(ai, options.dry_run, &mut commit_count) => {
                result?;
                true
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\n{}", "Received Ctrl+C, shutting down gracefully...".yellow());
                false
            }
        };

        if !should_continue {
            break;
        }

        if commit_count > old_count {
            commits_since_squash += 1;

            if options.rewrite_history && commits_since_squash >= options.squash_threshold {
                if let Err(e) = maybe_squash_commits(ai, options.squash_threshold, options.dry_run).await {
                    eprintln!("{} Squash failed: {}", "Warning:".yellow(), e);
                } else {
                    commits_since_squash = 0;
                }
            }
        }

        select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(options.interval)) => {}
            _ = tokio::signal::ctrl_c() => {
                println!("\n{}", "Received Ctrl+C, shutting down gracefully...".yellow());
                break;
            }
        }
    }

    println!("{} Auto mode stopped. {} commits made.",
        "✓".green(),
        commit_count.to_string().cyan()
    );

    Ok(())
}

async fn check_and_commit(ai: &AiClient, dry_run: bool, commit_count: &mut usize) -> Result<()> {
    let repo = git::open_repo(None)?;

    if git::has_uncommitted_changes(&repo)? {
        std::process::Command::new("git")
            .args(["add", "-A"])
            .output()
            .context("Failed to stage changes")?;

        let repo = git::open_repo(None)?;
        let changes = git::get_staged_changes(&repo)?;

        if !changes.is_empty() {
            let message = ai.generate_commit_message(&changes.diff, None, None).await?;

            if dry_run {
                println!("{} Would commit: {}",
                    "[DRY RUN]".yellow(),
                    message.lines().next().unwrap_or("")
                );
            } else {
                let oid = git::create_commit(&repo, &message, false)?;
                println!("{} Committed: {} - {}",
                    "✓".green(),
                    oid.to_string()[..7].cyan(),
                    message.lines().next().unwrap_or("")
                );
                *commit_count += 1;
            }
        }
    }

    Ok(())
}

async fn maybe_squash_commits(ai: &AiClient, count: usize, dry_run: bool) -> Result<()> {
    let repo = git::open_repo(None)?;

    let unpushed = git::count_unpushed_commits(&repo)?;
    if unpushed < count {
        println!("{} Only {} unpushed commits, need {} to squash. Skipping.",
            "→".dimmed(),
            unpushed,
            count
        );
        return Ok(());
    }

    let messages = git::get_commit_messages_for_squash(&repo, count)?;
    let commits_text = messages.join("\n---\n");

    let squash_message = ai.generate_squash_message(&commits_text).await?;

    if dry_run {
        println!("{} Would squash {} commits into:",
            "[DRY RUN]".yellow(),
            count
        );
        println!("  {}", squash_message.lines().next().unwrap_or(""));
        return Ok(());
    }

    let oid = git::squash_commits(&repo, count, &squash_message)?;

    println!("{} Squashed {} commits → {}",
        "⊕".cyan().bold(),
        count,
        oid.to_string()[..7].cyan()
    );
    println!("  {}", squash_message.lines().next().unwrap_or(""));

    Ok(())
}
