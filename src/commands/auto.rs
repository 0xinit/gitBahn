//! Auto command - Autonomous mode for watching and auto-committing.

use anyhow::{Context, Result};
use colored::Colorize;
use tokio::select;

use crate::config::Config;
use crate::core::ai::AiClient;
use crate::core::git;

/// Run the auto command
pub async fn run(
    config: &Config,
    watch: bool,
    interval: u64,
    merge: bool,
    target: &str,
    max_commits: usize,
    dry_run: bool,
) -> Result<()> {
    println!("{}", "gitBahn - Auto Mode".bold().cyan());
    println!();

    // Warn about unimplemented features
    if merge {
        println!("{} Auto-merge to '{}' is not yet implemented. Ignoring --merge flag.",
            "Warning:".yellow(), target);
        println!();
    }

    let api_key = config.anthropic_api_key()
        .context("ANTHROPIC_API_KEY not set")?;

    let ai = AiClient::new(api_key.to_string(), Some(config.ai.model.clone()));

    if watch {
        run_watch_mode(&ai, interval, max_commits, dry_run).await
    } else {
        run_single(&ai, dry_run).await
    }
}

async fn run_single(ai: &AiClient, dry_run: bool) -> Result<()> {
    let repo = git::open_repo(None)?;

    if !git::has_uncommitted_changes(&repo)? {
        println!("{}", "No changes to commit.".dimmed());
        return Ok(());
    }

    // Stage all changes
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

async fn run_watch_mode(ai: &AiClient, interval: u64, max_commits: usize, dry_run: bool) -> Result<()> {
    println!("Watching for changes every {}s (max {} commits)", interval, max_commits);
    println!("Press Ctrl+C to stop\n");

    let mut commit_count = 0;

    loop {
        if commit_count >= max_commits {
            println!("{}", "Max commits reached. Stopping.".yellow());
            break;
        }

        // Check for changes and commit if any
        let should_continue = select! {
            result = check_and_commit(ai, dry_run, &mut commit_count) => {
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

        // Wait for next interval, but also listen for Ctrl+C
        select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(interval)) => {}
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
        // Stage all changes
        std::process::Command::new("git")
            .args(["add", "-A"])
            .output()
            .context("Failed to stage changes")?;

        // Re-open to get fresh state
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
