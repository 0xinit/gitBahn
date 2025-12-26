//! Commit command - generate and create commits.

use anyhow::{Context, Result};
use colored::Colorize;
use dialoguer::{Confirm, Editor, Select};
use indicatif::{ProgressBar, ProgressStyle};

use crate::config::Config;
use crate::core::ai::AiClient;
use crate::core::git;

/// Options for the commit command
pub struct CommitOptions {
    pub atomic: bool,
    #[allow(dead_code)] // Will be used when custom templates are implemented
    pub conventional: bool,
    pub agent: Option<String>,
    pub auto_confirm: bool,
    pub verbose: bool,
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

    if options.atomic {
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
    let suggestions = ai.suggest_atomic_commits(&changes.diff, &files).await?;

    pb.finish_and_clear();

    if suggestions.len() == 1 {
        println!("{}", "Changes are already atomic (single logical unit).".yellow());
        // Fall back to single commit
        return run_single_commit(repo, changes, ai, context, personality, options).await;
    }

    println!("{} atomic commits suggested:\n", suggestions.len().to_string().cyan().bold());

    for (i, suggestion) in suggestions.iter().enumerate() {
        println!("{}. {}", (i + 1).to_string().bold(), suggestion.message.green());
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

        // Create the commit
        let oid = git::create_commit(&repo_fresh, &suggestion.message, false)?;
        created += 1;

        println!("  {} [{}/{}] {} - {}",
            "✓".green().bold(),
            created,
            total,
            oid.to_string()[..7].cyan(),
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
