//! Status command - Show repository status.

use anyhow::Result;
use colored::Colorize;

use crate::core::git;

/// Run the status command
pub fn run() -> Result<()> {
    println!("{}", "gitBahn - Status".bold().cyan());
    println!();

    let repo = git::open_repo(None)?;
    let branch = git::current_branch(&repo)?;
    let root = git::repo_root(&repo)?;

    println!("{} {}", "Repository:".bold(), root.display());
    println!("{} {}", "Branch:".bold(), branch.green());
    println!();

    // Check for staged changes
    let staged = git::get_staged_changes(&repo)?;

    if staged.is_empty() {
        println!("{}", "No staged changes.".dimmed());
    } else {
        println!("{}", "Staged changes:".bold());
        println!("  {} (+{}, -{})",
            staged.summary(),
            staged.stats.insertions.to_string().green(),
            staged.stats.deletions.to_string().red()
        );
        println!();

        if !staged.added.is_empty() {
            println!("  {}", "Added:".green());
            for file in &staged.added {
                println!("    + {}", file);
            }
        }

        if !staged.modified.is_empty() {
            println!("  {}", "Modified:".yellow());
            for file in &staged.modified {
                println!("    M {}", file);
            }
        }

        if !staged.deleted.is_empty() {
            println!("  {}", "Deleted:".red());
            for file in &staged.deleted {
                println!("    - {}", file);
            }
        }

        if !staged.renamed.is_empty() {
            println!("  {}", "Renamed:".blue());
            for (old, new) in &staged.renamed {
                println!("    {} → {}", old, new);
            }
        }
    }

    println!();

    // Check for uncommitted changes
    if git::has_uncommitted_changes(&repo)? {
        println!("{}", "You have uncommitted changes.".yellow());
        println!("Run {} to generate a commit message.", "bahn commit".cyan());
    } else {
        println!("{}", "Working tree clean.".green());
    }

    // Show recent commits
    let recent = git::get_recent_commits(&repo, 5)?;
    if !recent.is_empty() {
        println!();
        println!("{}", "Recent commits:".bold());
        for msg in recent {
            println!("  {} {}", "".dimmed(), msg);
        }
    }

    Ok(())
}
