//! Undo command for reverting commits.

use anyhow::{Context, Result};
use colored::Colorize;
use dialoguer::Confirm;

use crate::core::git;

/// Options for undo command
pub struct UndoOptions {
    /// Number of commits to undo
    pub count: usize,
    /// Hard reset (discard changes) vs soft reset (keep changes staged)
    pub hard: bool,
    /// Skip confirmation prompt
    pub yes: bool,
    /// Force undo even if commits are pushed
    pub force: bool,
}

impl Default for UndoOptions {
    fn default() -> Self {
        Self {
            count: 1,
            hard: false,
            yes: false,
            force: false,
        }
    }
}

/// Run the undo command
pub fn run(options: UndoOptions) -> Result<()> {
    let repo = git::open_repo(None)?;

    // Check if there are commits to undo
    let recent = git::get_recent_commits(&repo, options.count)?;
    if recent.is_empty() {
        println!("{} No commits to undo", "Info:".cyan());
        return Ok(());
    }

    // Check if commits have been pushed
    let unpushed = git::count_unpushed_commits(&repo)?;
    if unpushed < options.count && !options.force {
        println!(
            "{} Some commits have already been pushed to remote.",
            "Warning:".yellow()
        );
        println!("Only {} commits are unpushed, but you requested {}.", unpushed, options.count);
        println!("Use --force to undo anyway (will require force push).");
        return Ok(());
    }

    // Show what will be undone
    println!("{} Commits to undo:", "→".cyan());
    for (i, msg) in recent.iter().enumerate() {
        println!("  {}. {}", i + 1, msg);
    }
    println!();

    if options.hard {
        println!(
            "{} This will {} all changes in these commits!",
            "Warning:".yellow().bold(),
            "PERMANENTLY DELETE".red().bold()
        );
    } else {
        println!(
            "{} Changes will be unstaged but preserved in working directory.",
            "Note:".cyan()
        );
    }

    // Confirm unless --yes flag is set
    if !options.yes {
        let confirm = Confirm::new()
            .with_prompt("Proceed with undo?")
            .default(false)
            .interact()?;

        if !confirm {
            println!("{} Aborted", "→".yellow());
            return Ok(());
        }
    }

    // Perform the undo
    undo_commits(&repo, options.count, options.hard)?;

    println!(
        "{} Successfully undid {} commit{}",
        "✓".green(),
        options.count,
        if options.count == 1 { "" } else { "s" }
    );

    if !options.hard {
        println!("{} Your changes are preserved in the working directory.", "Tip:".cyan());
    }

    Ok(())
}

/// Undo commits by resetting HEAD
fn undo_commits(repo: &git2::Repository, count: usize, hard: bool) -> Result<()> {
    let head = repo.head()?;
    let head_commit = head.peel_to_commit()?;

    // Find the target commit (count commits back)
    let mut target = head_commit.clone();
    for _ in 0..count {
        target = target.parent(0)
            .context("Cannot undo: not enough commits in history")?;
    }

    // Reset to target
    let reset_type = if hard {
        git2::ResetType::Hard
    } else {
        git2::ResetType::Mixed
    };

    repo.reset(target.as_object(), reset_type, None)?;

    Ok(())
}

/// Show what the last N commits are (for preview)
pub fn preview(count: usize) -> Result<()> {
    let repo = git::open_repo(None)?;
    let recent = git::get_recent_commits(&repo, count)?;

    if recent.is_empty() {
        println!("{} No commits in history", "Info:".cyan());
        return Ok(());
    }

    println!("{} Last {} commit{}:", "→".cyan(), count, if count == 1 { "" } else { "s" });
    for (i, msg) in recent.iter().enumerate() {
        println!("  {}. {}", i + 1, msg);
    }

    let unpushed = git::count_unpushed_commits(&repo)?;
    println!();
    println!(
        "{} {} commit{} can be safely undone (not pushed)",
        "Info:".cyan(),
        unpushed,
        if unpushed == 1 { "" } else { "s" }
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_undo_options_default() {
        let opts = UndoOptions::default();
        assert_eq!(opts.count, 1);
        assert!(!opts.hard);
        assert!(!opts.yes);
        assert!(!opts.force);
    }
}
