//! Review command - AI-powered code review.

use anyhow::{Context, Result};
use colored::Colorize;

use crate::config::Config;
use crate::core::ai::AiClient;
use crate::core::git;

/// Run the review command
pub async fn run(config: &Config, staged: bool, commit: Option<&str>, strictness: &str) -> Result<()> {
    println!("{}", "gitBahn - Code Review".bold().cyan());
    println!();

    let api_key = config.anthropic_api_key()
        .context("ANTHROPIC_API_KEY not set")?;

    let ai = AiClient::new(api_key.to_string(), Some(config.ai.model.clone()));
    let repo = git::open_repo(None)?;

    let diff = if let Some(commit_sha) = commit {
        get_commit_diff(&repo, commit_sha)?
    } else if staged {
        let changes = git::get_staged_changes(&repo)?;
        if changes.is_empty() {
            println!("{}", "No staged changes to review.".yellow());
            return Ok(());
        }
        changes.diff
    } else {
        // Default to staged changes
        let changes = git::get_staged_changes(&repo)?;
        if changes.is_empty() {
            println!("{}", "No staged changes to review.".yellow());
            println!("Stage changes with: git add <files>");
            return Ok(());
        }
        changes.diff
    };

    println!("{}", "Analyzing code...".dimmed());

    let review = ai.review_code(&diff, None, None, strictness).await?;

    // Display review results
    println!();
    println!("{} {}", "Verdict:".bold(), format_verdict(&review.verdict));
    println!("{} {}/10", "Score:".bold(), review.overall_score);
    println!();

    println!("{}", "Summary:".bold());
    println!("  {}", review.summary);
    println!();

    if !review.issues.is_empty() {
        println!("{}", "Issues:".bold().red());
        for issue in &review.issues {
            let severity_color = match issue.severity.as_str() {
                "critical" => "".red().bold(),
                "warning" => "".yellow(),
                _ => "".dimmed(),
            };
            println!("  {} [{}] {}:{}",
                severity_color,
                issue.severity.to_uppercase(),
                issue.file,
                issue.line.map(|l| l.to_string()).unwrap_or_default()
            );
            println!("    {}", issue.message);
            if let Some(suggestion) = &issue.suggestion {
                println!("    {} {}", "Suggestion:".dimmed(), suggestion);
            }
        }
        println!();
    }

    if !review.positives.is_empty() {
        println!("{}", "Positives:".bold().green());
        for positive in &review.positives {
            println!("  {} {}", "".green(), positive);
        }
    }

    Ok(())
}

fn format_verdict(verdict: &str) -> colored::ColoredString {
    match verdict {
        "approve" => "APPROVED".green().bold(),
        "request_changes" => "CHANGES REQUESTED".red().bold(),
        _ => verdict.yellow(),
    }
}

fn get_commit_diff(repo: &git2::Repository, commit_sha: &str) -> Result<String> {
    let oid = git2::Oid::from_str(commit_sha)
        .with_context(|| format!("Invalid commit SHA: {}", commit_sha))?;

    let commit = repo.find_commit(oid)?;
    let tree = commit.tree()?;

    let parent_tree = if commit.parent_count() > 0 {
        Some(commit.parent(0)?.tree()?)
    } else {
        None
    };

    let diff = repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None)?;

    let mut diff_text = String::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        let prefix = match line.origin() {
            '+' => "+",
            '-' => "-",
            ' ' => " ",
            _ => "",
        };
        if !prefix.is_empty() {
            diff_text.push_str(prefix);
        }
        if let Ok(content) = std::str::from_utf8(line.content()) {
            diff_text.push_str(content);
        }
        true
    })?;

    Ok(diff_text)
}
