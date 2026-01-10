//! Push command with optional PR creation.

use std::process::Command;

use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::core::git;

/// Options for push command
pub struct PushOptions {
    /// Create a pull request after pushing
    pub create_pr: bool,
    /// PR title (auto-generated if not provided)
    pub title: Option<String>,
    /// PR body (auto-generated if not provided)
    pub body: Option<String>,
    /// Target branch for PR (default: main)
    pub base: String,
    /// Draft PR
    pub draft: bool,
    /// Force push
    pub force: bool,
    /// Set upstream
    pub set_upstream: bool,
}

impl Default for PushOptions {
    fn default() -> Self {
        Self {
            create_pr: false,
            title: None,
            body: None,
            base: "main".to_string(),
            draft: false,
            force: false,
            set_upstream: true,
        }
    }
}

/// GitHub PR creation request
#[derive(Debug, Serialize)]
struct CreatePrRequest {
    title: String,
    body: String,
    head: String,
    base: String,
    draft: bool,
}

/// GitHub PR response
#[derive(Debug, Deserialize)]
struct PrResponse {
    #[allow(dead_code)]
    number: u64,
    html_url: String,
}

/// Run the push command
pub async fn run(config: &Config, options: PushOptions) -> Result<()> {
    let repo = git::open_repo(None)?;
    let branch = git::current_branch(&repo)?;

    // Check if on protected branch
    if is_protected_branch(&branch) && !options.force {
        println!(
            "{} You're on '{}'. Consider using a feature branch.",
            "Warning:".yellow(),
            branch
        );
    }

    // Push to remote
    println!("{} Pushing to remote...", "→".cyan());
    push_to_remote(&branch, options.force, options.set_upstream)?;
    println!("{} Pushed successfully", "✓".green());

    // Create PR if requested
    if options.create_pr {
        let token = config.github_token()
            .context("GitHub token required for PR creation. Set GITHUB_TOKEN env var or add to .bahn.toml")?;

        println!("{} Creating pull request...", "→".cyan());

        let pr_url = create_pull_request(
            token,
            &branch,
            &options.base,
            options.title,
            options.body,
            options.draft,
            &repo,
        ).await?;

        println!("{} Pull request created: {}", "✓".green(), pr_url.cyan());
    }

    Ok(())
}

/// Push to remote
fn push_to_remote(branch: &str, force: bool, set_upstream: bool) -> Result<()> {
    let mut args = vec!["push"];

    if set_upstream {
        args.push("-u");
        args.push("origin");
    }

    args.push(branch);

    if force {
        args.push("--force-with-lease");
    }

    let output = Command::new("git")
        .args(&args)
        .output()
        .context("Failed to execute git push")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Git push failed: {}", stderr);
    }

    Ok(())
}

/// Create a pull request using GitHub API