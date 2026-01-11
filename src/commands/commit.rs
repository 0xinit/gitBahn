//! Commit command - generate and create commits.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Local, NaiveDateTime, TimeZone};
use colored::Colorize;
use dialoguer::{Confirm, Editor, Select};
use indicatif::{ProgressBar, ProgressStyle};
use rand::Rng;

use crate::config::Config;
use crate::core::ai::{AiClient, ChunkInfo, HunkInfo};
use crate::core::git;
use crate::core::secrets;

/// Options for the commit command
pub struct CommitOptions {
    pub atomic: bool,
    /// Target number of commits to split into
    pub split: Option<usize>,
    /// Split individual files into hunks for ultra-realistic commits
    pub granular: bool,
    /// Realistic mode - simulate human development flow
    pub realistic: bool,
    #[allow(dead_code)] // Will be used when custom templates are implemented
    pub conventional: bool,
    pub agent: Option<String>,
    pub auto_confirm: bool,
    pub verbose: bool,
    /// Spread atomic commits over time (e.g., "2h", "30m", "1d")
    pub spread: Option<String>,
    /// Start time for atomic commits (e.g., "2025-12-25 09:00")
    pub start: Option<String>,
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
        // Default to hours if no unit
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
    // Try parsing with time
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M") {
        return Local.from_local_datetime(&naive).single()
            .context("Invalid local datetime");
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Local.from_local_datetime(&naive).single()
            .context("Invalid local datetime");
    }
    // Try parsing date only (use 9:00 AM as default)
    if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let naive = date.and_hms_opt(9, 0, 0).context("Invalid time")?;
        return Local.from_local_datetime(&naive).single()
            .context("Invalid local datetime");
    }

    anyhow::bail!("Invalid datetime format: {}. Use YYYY-MM-DD HH:MM", s)
}

/// Generate realistic timestamps for commits spread over a duration
/// Returns timestamps with random gaps that look like natural coding sessions
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

    // Calculate base interval between commits
    let base_interval = total_duration_secs / (count as i64);

    // Generate timestamps with some randomness
    let mut current = start;
    for i in 0..count {
        timestamps.push(current);

        if i < count - 1 {
            // Add some variance: 50% to 150% of base interval
            let variance = rng.gen_range(0.5..1.5);
            let interval = (base_interval as f64 * variance) as i64;

            // Add random seconds for human-like timestamps (not round minutes)
            let extra_secs = rng.gen_range(0..60);

            current += Duration::seconds(interval.max(60) + extra_secs);
        }
    }

    // If we overshot, scale back proportionally
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

/// Generate default realistic spread (2-4 hours like a coding session)
fn default_spread_duration() -> i64 {
    let mut rng = rand::thread_rng();
    rng.gen_range(2..=4) * 3600 // 2-4 hours in seconds
}

/// Run the commit command
pub async fn run(options: CommitOptions, config: &Config) -> Result<()> {
    // Open repository
    let repo = git::open_repo(None)?;
    let branch = git::current_branch(&repo)?;

    println!("{} on branch {}\n", "bahn commit".bold(), branch.cyan());

    // Get staged changes
    let changes = git::get_staged_changes(&repo)?;

    if changes.is_empty() {
        println!("{}", "No staged changes to commit.".yellow());
        println!("Stage changes with: git add <files>");
        return Ok(());
    }

    // Show summary
    println!("{}", "Staged changes:".bold());
    println!("  {} (+{}, -{})",
        changes.summary(),
        changes.stats.insertions.to_string().green(),
        changes.stats.deletions.to_string().red()
    );
    println!();

    if options.verbose {
        println!("{}", "Files:".bold());
        for file in &changes.added {
            println!("  {} {}", "+".green(), file);
        }
        for file in &changes.modified {
            println!("  {} {}", "M".yellow(), file);
        }
        for file in &changes.deleted {
            println!("  {} {}", "-".red(), file);
        }
        for (old, new) in &changes.renamed {
            println!("  {} {} → {}", "R".blue(), old, new);
        }
        println!();
    }

    // Branch awareness - warn if on protected branch
    if is_protected_branch(&branch) {
        println!(
            "{} You are committing directly to '{}'. Consider using a feature branch.",
            "Warning:".yellow().bold(),
            branch.cyan()
        );
        if !options.auto_confirm {
            let proceed = dialoguer::Confirm::new()
                .with_prompt("Continue anyway?")
                .default(false)
                .interact()?;
            if !proceed {
                println!("{}", "Commit cancelled.".yellow());
                return Ok(());
            }
        }
        println!();
    }

    // Secret detection - scan for potential secrets in staged changes
    let detected_secrets = secrets::check_diff_for_secrets(&changes.diff);
    let high_confidence_secrets: Vec<_> = detected_secrets.iter()
        .filter(|s| s.confidence >= 0.7)
        .collect();

    if !high_confidence_secrets.is_empty() {
        println!("{}", secrets::format_secret_warnings(&high_confidence_secrets.iter().cloned().cloned().collect::<Vec<_>>()));

        if !options.auto_confirm {
            println!(
                "{} Found {} potential secret(s) in staged changes!",
                "Security:".red().bold(),
                high_confidence_secrets.len()
            );
            let proceed = dialoguer::Confirm::new()
                .with_prompt("Commit anyway? (Not recommended)")
                .default(false)
                .interact()?;
            if !proceed {
                println!("{}", "Commit cancelled. Please remove secrets before committing.".yellow());
                return Ok(());
            }
        } else {
            // In auto mode, refuse to commit secrets
            anyhow::bail!(
                "Refusing to auto-commit: {} potential secret(s) detected. Use interactive mode to override.",
                high_confidence_secrets.len()
            );
        }
        println!();
    }

    // Get API key
    let api_key = config.anthropic_api_key()
        .context("ANTHROPIC_API_KEY not set. Run: export ANTHROPIC_API_KEY=your_key")?;

    let ai = AiClient::new(api_key.to_string(), Some(config.ai.model.clone()));

    // Get recent commits for context
    let recent = git::get_recent_commits(&repo, 5)?;
    let context = if recent.is_empty() {
        None
    } else {
        Some(format!("Recent commits:\n{}", recent.iter()
            .map(|m| format!("  - {}", m))
            .collect::<Vec<_>>()
            .join("\n")))
    };

    let personality = options.agent.as_deref()
        .or(config.commit.default_agent.as_deref());

    if options.realistic {
        run_realistic_commits(&repo, &ai, &options).await
    } else if options.granular {
        run_granular_commits(&repo, &changes, &ai, context.as_deref(), personality, &options).await
    } else if options.atomic {
        run_atomic_commits(&repo, &changes, &ai, context.as_deref(), personality, &options).await
    } else {
        run_single_commit(&repo, &changes, &ai, context.as_deref(), personality, &options).await
    }
}

async fn run_single_commit(
    repo: &git2::Repository,
    changes: &git::StagedChanges,
    ai: &AiClient,
    context: Option<&str>,
    personality: Option<&str>,
    options: &CommitOptions,
) -> Result<()> {
    // Show progress
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::default_spinner()
        .template("{spinner:.green} {msg}")
        .unwrap());
    pb.set_message("Generating commit message...");

    // Generate commit message
    let message = ai.generate_commit_message(&changes.diff, context, personality).await?;

    pb.finish_and_clear();

    println!("{}", "Generated commit message:".bold());
    println!("{}", "─".repeat(50).dimmed());
    println!("{}", message);
    println!("{}", "─".repeat(50).dimmed());
    println!();

    // Confirm or edit
    let final_message = if options.auto_confirm {
        message
    } else {
        let choices = vec!["Accept", "Edit", "Cancel"];
        let selection = Select::new()
            .with_prompt("What would you like to do?")
            .items(&choices)
            .default(0)
            .interact()?;

        match selection {
            0 => message,
            1 => {
                // Open editor
                let edited = Editor::new()
                    .edit(&message)?
                    .context("Editor returned empty message")?;
                edited.trim().to_string()
            }
            _ => {
                println!("{}", "Commit cancelled.".yellow());
                return Ok(());
            }
        }
    };

    // Create commit
    let oid = git::create_commit(repo, &final_message, false)?;

    println!();
    println!("{} Created commit {}",
        "✓".green().bold(),
        oid.to_string()[..7].cyan()
    );
    println!("  {}", final_message.lines().next().unwrap_or(""));

    Ok(())
}

async fn run_atomic_commits(
    repo: &git2::Repository,
    changes: &git::StagedChanges,
    ai: &AiClient,
    context: Option<&str>,
    personality: Option<&str>,
    options: &CommitOptions,
) -> Result<()> {
    // Show progress
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::default_spinner()
        .template("{spinner:.green} {msg}")
        .unwrap());
    pb.set_message("Analyzing changes for atomic commits...");

    // Get atomic commit suggestions
    let files: Vec<&str> = changes.all_files();
    let suggestions = ai.suggest_atomic_commits(&changes.diff, &files, options.split).await?;

    pb.finish_and_clear();

    if suggestions.len() == 1 {
        println!("{}", "Changes are already atomic (single logical unit).".yellow());
        // Fall back to single commit
        return run_single_commit(repo, changes, ai, context, personality, options).await;
    }

    // Generate timestamps for commits
    let start_time = if let Some(ref start_str) = options.start {
        parse_start_time(start_str)?
    } else {
        Local::now()
    };

    let spread_duration = if let Some(ref spread_str) = options.spread {
        parse_duration(spread_str)?
    } else {
        default_spread_duration()
    };

    let timestamps = generate_spread_timestamps(suggestions.len(), start_time, spread_duration);

    println!("{} atomic commits suggested:\n", suggestions.len().to_string().cyan().bold());

    for (i, suggestion) in suggestions.iter().enumerate() {
        let ts_str = timestamps.get(i)
            .map(|t| t.format("%b %d, %H:%M:%S").to_string())
            .unwrap_or_default();
        println!("{}. {} → {}",
            (i + 1).to_string().bold(),
            suggestion.message.green(),
            ts_str.dimmed()
        );
        println!("   Files: {}", suggestion.files.join(", ").dimmed());
        println!("   {}", suggestion.description.dimmed());
        println!();
    }

    // Ask for confirmation unless auto_confirm is set
    let proceed = if options.auto_confirm {
        true
    } else {
        let choices = vec!["Create all atomic commits", "Create single commit instead", "Cancel"];
        let selection = Select::new()
            .with_prompt("What would you like to do?")
            .items(&choices)
            .default(0)
            .interact()?;

        match selection {
            0 => true,  // Proceed with atomic commits
            1 => {
                // Fall back to single commit
                return run_single_commit(repo, changes, ai, context, personality, options).await;
            }
            _ => {
                println!("{}", "Commit cancelled.".yellow());
                return Ok(());
            }
        }
    };

    if !proceed {
        return Ok(());
    }

    // Reset staging area first
    git::reset_index(repo)?;

    let total = suggestions.len();
    let mut created = 0;

    println!("\n{}", "Creating atomic commits...".bold());

    for (i, suggestion) in suggestions.iter().enumerate() {
        // Stage only the files for this commit
        let file_refs: Vec<&str> = suggestion.files.iter().map(|s| s.as_str()).collect();

        // Some files might not exist in working tree (AI hallucination), filter them
        let valid_files: Vec<&str> = file_refs.iter()
            .filter(|f| {
                let all_files = changes.all_files();
                all_files.contains(f)
            })
            .copied()
            .collect();

        if valid_files.is_empty() {
            println!("  {} Skipping group {}/{}: no valid files",
                "→".dimmed(),
                i + 1,
                total
            );
            continue;
        }

        git::stage_files(repo, &valid_files)?;

        // Verify something is staged
        let repo_fresh = git::open_repo(None)?;
        let staged = git::get_staged_changes(&repo_fresh)?;

        if staged.is_empty() {
            println!("  {} Skipping group {}/{}: nothing staged",
                "→".dimmed(),
                i + 1,
                total
            );
            continue;
        }

        // Create the commit with timestamp
        let commit_time = timestamps.get(i).copied();
        let oid = git::create_commit_at(&repo_fresh, &suggestion.message, false, commit_time)?;
        created += 1;

        let ts_str = commit_time
            .map(|t| t.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "now".to_string());
        println!("  {} [{}/{}] {} @ {} - {}",
            "✓".green().bold(),
            created,
            total,
            oid.to_string()[..7].cyan(),
            ts_str.dimmed(),
            suggestion.message.lines().next().unwrap_or("")
        );
    }

    // Check if there are any remaining unstaged changes
    let repo_final = git::open_repo(None)?;
    if git::has_uncommitted_changes(&repo_final)? {
        println!("\n{} Some files weren't included in atomic groups.",
            "Note:".yellow()
        );

        let confirm = Confirm::new()
            .with_prompt("Commit remaining changes?")
            .default(true)
            .interact()?;

        if confirm {
            git::stage_all(&repo_final)?;
            let remaining = git::get_staged_changes(&repo_final)?;

            if !remaining.is_empty() {
                let message = ai.generate_commit_message(&remaining.diff, context, personality).await?;
                let oid = git::create_commit(&repo_final, &message, false)?;
                created += 1;

                println!("  {} [{}/{}] {} - {}",
                    "✓".green().bold(),
                    created,
                    total + 1,
                    oid.to_string()[..7].cyan(),
                    message.lines().next().unwrap_or("")
                );
            }
        }
    }

    println!("\n{} Created {} atomic commits.",
        "✓".green().bold(),
        created.to_string().cyan()
    );

    Ok(())
}

async fn run_granular_commits(
    repo: &git2::Repository,
    changes: &git::StagedChanges,
    ai: &AiClient,
    _context: Option<&str>,
    _personality: Option<&str>,
    options: &CommitOptions,
) -> Result<()> {
    // Show progress
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::default_spinner()
        .template("{spinner:.green} {msg}")
        .unwrap());
    pb.set_message("Parsing diff into hunks...");

    // Parse the diff into individual hunks
    let hunks = git::parse_diff_into_hunks(&changes.diff);

    if hunks.is_empty() {
        pb.finish_and_clear();
        println!("{}", "No hunks found in staged changes.".yellow());
        return Ok(());
    }

    pb.set_message(format!("Found {} hunks, analyzing...", hunks.len()));

    // Convert to HunkInfo for AI
    let hunk_infos: Vec<HunkInfo> = hunks.iter().map(|h| {
        // Create a preview of the content (first 100 chars of added lines)
        let preview: String = h.content.lines()
            .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
            .take(2)
            .map(|l| l.trim_start_matches('+').trim())
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(100)
            .collect();

        HunkInfo {
            id: h.id,
            file_path: h.file_path.clone(),
            is_new_file: h.is_new_file,
            is_deleted: h.is_deleted,
            additions: h.additions,
            deletions: h.deletions,
            context: h.context.clone(),
            content_preview: preview,
        }
    }).collect();

    // Get AI suggestions for grouping hunks
    let suggestions = ai.suggest_granular_commits(&hunk_infos, options.split).await?;

    pb.finish_and_clear();

    if suggestions.is_empty() {
        println!("{}", "No commit suggestions generated.".yellow());
        return Ok(());
    }

    // Generate timestamps for commits
    let start_time = if let Some(ref start_str) = options.start {
        parse_start_time(start_str)?
    } else {
        Local::now()
    };

    let spread_duration = if let Some(ref spread_str) = options.spread {
        parse_duration(spread_str)?
    } else {
        default_spread_duration()
    };

    let timestamps = generate_spread_timestamps(suggestions.len(), start_time, spread_duration);

    println!("{} granular commits suggested (from {} hunks):\n",
        suggestions.len().to_string().cyan().bold(),
        hunks.len()
    );

    for (i, suggestion) in suggestions.iter().enumerate() {
        let ts_str = timestamps.get(i)
            .map(|t| t.format("%b %d, %H:%M:%S").to_string())
            .unwrap_or_default();

        // Show which files/hunks are involved
        let hunk_files: Vec<&str> = suggestion.hunk_ids.iter()
            .filter_map(|id| hunks.iter().find(|h| h.id == *id))
            .map(|h| h.file_path.as_str())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        println!("{}. {} → {}",
            (i + 1).to_string().bold(),
            suggestion.message.green(),
            ts_str.dimmed()
        );
        println!("   Hunks: {} | Files: {}",
            suggestion.hunk_ids.iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ")
                .dimmed(),
            hunk_files.join(", ").dimmed()
        );
        println!("   {}", suggestion.description.dimmed());
        println!();
    }

    // Ask for confirmation unless auto_confirm is set
    let proceed = if options.auto_confirm {
        true
    } else {
        let choices = vec!["Create all granular commits", "Cancel"];
        let selection = Select::new()
            .with_prompt("What would you like to do?")
            .items(&choices)
            .default(0)
            .interact()?;

        selection == 0
    };

    if !proceed {
        println!("{}", "Commit cancelled.".yellow());
        return Ok(());
    }

    // Reset staging area first
    git::reset_index(repo)?;

    let total = suggestions.len();
    let mut created = 0;

    println!("\n{}", "Creating granular commits...".bold());

    let repo_path = repo.workdir()
        .context("Repository has no working directory")?;

    for (i, suggestion) in suggestions.iter().enumerate() {
        // Get the hunks for this commit
        let commit_hunks: Vec<&git::DiffHunk> = suggestion.hunk_ids.iter()
            .filter_map(|id| hunks.iter().find(|h| h.id == *id))
            .collect();

        if commit_hunks.is_empty() {
            println!("  {} Skipping commit {}/{}: no valid hunks",
                "→".dimmed(),
                i + 1,
                total
            );
            continue;
        }

        // Stage the hunks for this commit
        git::stage_hunks(repo_path, &commit_hunks)?;

        // Verify something is staged
        let repo_fresh = git::open_repo(None)?;
        let staged = git::get_staged_changes(&repo_fresh)?;

        if staged.is_empty() {
            println!("  {} Skipping commit {}/{}: nothing staged",
                "→".dimmed(),
                i + 1,
                total
            );
            continue;
        }

        // Create the commit with timestamp
        let commit_time = timestamps.get(i).copied();
        let oid = git::create_commit_at(&repo_fresh, &suggestion.message, false, commit_time)?;
        created += 1;

        let ts_str = commit_time
            .map(|t| t.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "now".to_string());
        println!("  {} [{}/{}] {} @ {} - {}",
            "✓".green().bold(),
            created,
            total,
            oid.to_string()[..7].cyan(),
            ts_str.dimmed(),
            suggestion.message.lines().next().unwrap_or("")
        );
    }

    // Check if there are any remaining unstaged changes
    let repo_final = git::open_repo(None)?;
    if git::has_uncommitted_changes(&repo_final)? {
        println!("\n{} Some hunks weren't included in commits.",
            "Note:".yellow()
        );

        let confirm = Confirm::new()
            .with_prompt("Commit remaining changes?")
            .default(true)
            .interact()?;

        if confirm {
            git::stage_all(&repo_final)?;
            let remaining = git::get_staged_changes(&repo_final)?;

            if !remaining.is_empty() {
                let message = ai.generate_commit_message(&remaining.diff, None, None).await?;
                let oid = git::create_commit(&repo_final, &message, false)?;
                created += 1;

                println!("  {} [{}/{}] {} - {}",
                    "✓".green().bold(),
                    created,
                    total + 1,
                    oid.to_string()[..7].cyan(),
                    message.lines().next().unwrap_or("")
                );
            }
        }
    }

    println!("\n{} Created {} granular commits.",
        "✓".green().bold(),
        created.to_string().cyan()
    );

    Ok(())
}

async fn run_realistic_commits(
    repo: &git2::Repository,
    ai: &AiClient,
    options: &CommitOptions,
) -> Result<()> {
    let repo_path = repo.workdir()
        .context("Repository has no working directory")?;

    // Show progress
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::default_spinner()
        .template("{spinner:.green} {msg}")
        .unwrap());
    pb.set_message("Analyzing files for realistic commit flow...");

    // Parse files into logical chunks
    let chunked = git::parse_files_into_chunks(repo)?;

    if chunked.chunks.is_empty() {
        pb.finish_and_clear();
        println!("{}", "No files to commit.".yellow());
        return Ok(());
    }

    pb.set_message(format!("Found {} chunks across {} files, planning commits...",
        chunked.chunks.len(),
        chunked.file_order.len()
    ));

    // Convert to ChunkInfo for AI
    let chunk_infos: Vec<ChunkInfo> = chunked.chunks.iter().map(|c| {
        // Create preview
        let preview: String = c.content.lines()
            .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#') && !l.trim().starts_with("//"))
            .take(2)
            .map(|l| l.trim())
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(80)
            .collect();

        ChunkInfo {
            id: c.id,
            file_path: c.file_path.clone(),
            start_line: c.start_line,
            end_line: c.end_line,
            line_count: c.line_count,
            chunk_type: c.chunk_type.to_string(),
            description: c.description.clone(),
            content_preview: preview,
            is_new_file: true, // For now, assume all are new
        }
    }).collect();

    // Get AI to plan the commits
    let commit_plans = ai.plan_realistic_commits(&chunk_infos, &chunked.file_order, options.split).await?;

    pb.finish_and_clear();

    if commit_plans.is_empty() {
        println!("{}", "No commit plan generated.".yellow());
        return Ok(());
    }

    // Generate timestamps
    let start_time = if let Some(ref start_str) = options.start {
        parse_start_time(start_str)?
    } else {
        Local::now()
    };

    let spread_duration = if let Some(ref spread_str) = options.spread {
        parse_duration(spread_str)?
    } else {
        default_spread_duration()
    };

    let timestamps = generate_spread_timestamps(commit_plans.len(), start_time, spread_duration);

    println!("{} realistic commits planned (from {} chunks in {} files):\n",
        commit_plans.len().to_string().cyan().bold(),
        chunked.chunks.len(),
        chunked.file_order.len()
    );

    for (i, plan) in commit_plans.iter().enumerate() {
        let ts_str = timestamps.get(i)
            .map(|t| t.format("%b %d, %H:%M:%S").to_string())
            .unwrap_or_default();

        // Show files involved
        let files: Vec<&str> = plan.chunk_ids.iter()
            .filter_map(|id| chunked.chunks.iter().find(|c| c.id == *id))
            .map(|c| c.file_path.as_str())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        println!("{}. {} → {}",
            (i + 1).to_string().bold(),
            plan.message.green(),
            ts_str.dimmed()
        );
        println!("   Chunks: {} | Files: {}",
            plan.chunk_ids.iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ")
                .dimmed(),
            files.join(", ").dimmed()
        );
        println!("   {}", plan.description.dimmed());
        println!();
    }

    // Ask for confirmation
    let proceed = if options.auto_confirm {
        true
    } else {
        let choices = vec!["Create all realistic commits", "Cancel"];
        let selection = Select::new()
            .with_prompt("What would you like to do?")
            .items(&choices)
            .default(0)
            .interact()?;

        selection == 0
    };

    if !proceed {
        println!("{}", "Commit cancelled.".yellow());
        return Ok(());
    }

    // First, save original file contents and reset staging
    let mut original_contents: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for chunk in &chunked.chunks {
        if !original_contents.contains_key(&chunk.file_path) {
            let full_path = repo_path.join(&chunk.file_path);
            if let Ok(content) = std::fs::read_to_string(&full_path) {
                original_contents.insert(chunk.file_path.clone(), content);
            }
        }
    }

    // Reset staging and working directory for progressive building
    git::reset_index(repo)?;

    // Track which chunks have been committed
    let mut committed_chunks: std::collections::HashSet<usize> = std::collections::HashSet::new();
    // Track cumulative content for each file
    let mut file_contents: std::collections::HashMap<String, Vec<&git::FileChunk>> = std::collections::HashMap::new();

    let total = commit_plans.len();
    let mut created = 0;

    println!("\n{}", "Creating realistic commits...".bold());

    for (i, plan) in commit_plans.iter().enumerate() {
        // Get chunks for this commit
        let commit_chunks: Vec<&git::FileChunk> = plan.chunk_ids.iter()
            .filter_map(|id| chunked.chunks.iter().find(|c| c.id == *id))
            .filter(|c| !committed_chunks.contains(&c.id))
            .collect();

        if commit_chunks.is_empty() {
            println!("  {} Skipping commit {}/{}: no valid chunks",
                "→".dimmed(),
                i + 1,
                total
            );
            continue;
        }

        // Add chunks to cumulative file contents
        for chunk in &commit_chunks {
            file_contents.entry(chunk.file_path.clone())
                .or_default()
                .push(chunk);
            committed_chunks.insert(chunk.id);
        }

        // Write cumulative content for each affected file
        let affected_files: Vec<String> = commit_chunks.iter()
            .map(|c| c.file_path.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        for file_path in &affected_files {
            if let Some(chunks) = file_contents.get(file_path) {
                // Sort chunks by start_line and build cumulative content
                let mut sorted_chunks: Vec<&&git::FileChunk> = chunks.iter().collect();
                sorted_chunks.sort_by_key(|c| c.start_line);

                let content: String = sorted_chunks.iter()
                    .map(|c| c.content.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");

                // Write the file with cumulative content
                git::write_file_content(repo_path, file_path, &content)?;
                git::stage_file(repo_path, file_path)?;
            }
        }

        // Verify something is staged
        let repo_fresh = git::open_repo(None)?;
        let staged = git::get_staged_changes(&repo_fresh)?;

        if staged.is_empty() {
            println!("  {} Skipping commit {}/{}: nothing staged",
                "→".dimmed(),
                i + 1,
                total
            );
            continue;
        }

        // Create the commit
        let commit_time = timestamps.get(i).copied();
        let oid = git::create_commit_at(&repo_fresh, &plan.message, false, commit_time)?;
        created += 1;

        let ts_str = commit_time
            .map(|t| t.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "now".to_string());
        println!("  {} [{}/{}] {} @ {} - {}",
            "✓".green().bold(),
            created,
            total,
            oid.to_string()[..7].cyan(),
            ts_str.dimmed(),
            plan.message.lines().next().unwrap_or("")
        );
    }

    // Restore any files that weren't fully committed
    for (file_path, original) in &original_contents {
        let full_path = repo_path.join(file_path);
        let current = std::fs::read_to_string(&full_path).unwrap_or_default();
        if current != *original {
            // File is not complete, restore original
            std::fs::write(&full_path, original)?;
        }
    }

    // Check for remaining uncommitted content
    let repo_final = git::open_repo(None)?;
    if git::has_uncommitted_changes(&repo_final)? {
        println!("\n{} Some content wasn't included in commits.",
            "Note:".yellow()
        );

        let confirm = Confirm::new()
            .with_prompt("Commit remaining changes?")
            .default(true)
            .interact()?;

        if confirm {
            git::stage_all(&repo_final)?;
            let remaining = git::get_staged_changes(&repo_final)?;

            if !remaining.is_empty() {
                let message = ai.generate_commit_message(&remaining.diff, None, None).await?;
                let oid = git::create_commit(&repo_final, &message, false)?;
                created += 1;

                println!("  {} [{}/{}] {} - {}",
                    "✓".green().bold(),
                    created,
                    total + 1,
                    oid.to_string()[..7].cyan(),
                    message.lines().next().unwrap_or("")
                );
            }
        }
    }

    println!("\n{} Created {} realistic commits.",
        "✓".green().bold(),
        created.to_string().cyan()
    );

    Ok(())
}

/// Check if the current branch is a protected branch
fn is_protected_branch(branch: &str) -> bool {
    matches!(
        branch.to_lowercase().as_str(),
        "main" | "master" | "develop" | "development" | "production" | "staging" | "release"
    )
}
