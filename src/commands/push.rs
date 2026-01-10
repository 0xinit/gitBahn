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
async fn create_pull_request(
    token: &str,
    head: &str,
    base: &str,
    title: Option<String>,
    body: Option<String>,
    draft: bool,
    repo: &git2::Repository,
) -> Result<String> {
    // Get repository info from remote URL
    let (owner, repo_name) = get_repo_info(repo)?;

    // Generate title from branch name or commits if not provided
    let title = title.unwrap_or_else(|| generate_pr_title(head));

    // Generate body from commits if not provided
    let body = body.unwrap_or_else(|| generate_pr_body(repo, base).unwrap_or_default());

    let request = CreatePrRequest {
        title,
        body,
        head: head.to_string(),
        base: base.to_string(),
        draft,
    };

    let client = reqwest::Client::new();
    let url = format!("https://api.github.com/repos/{}/{}/pulls", owner, repo_name);

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "gitBahn")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .json(&request)
        .send()
        .await
        .context("Failed to send PR request")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!("GitHub API error ({}): {}", status, error_text);
    }

    let pr: PrResponse = response.json().await
        .context("Failed to parse PR response")?;

    Ok(pr.html_url)
}

/// Get owner and repo name from git remote
fn get_repo_info(repo: &git2::Repository) -> Result<(String, String)> {
    let remote = repo.find_remote("origin")
        .context("No 'origin' remote found")?;

    let url = remote.url()
        .context("Could not get remote URL")?;

    parse_github_url(url)
}

/// Parse GitHub URL to extract owner and repo
fn parse_github_url(url: &str) -> Result<(String, String)> {
    // Handle SSH format: git@github.com:owner/repo.git
    if url.starts_with("git@github.com:") {
        let path = url.trim_start_matches("git@github.com:");
        let path = path.trim_end_matches(".git");
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() >= 2 {
            return Ok((parts[0].to_string(), parts[1].to_string()));
        }
    }

    // Handle HTTPS format: https://github.com/owner/repo.git
    if url.contains("github.com") {
        let path = url
            .trim_start_matches("https://github.com/")
            .trim_start_matches("http://github.com/")
            .trim_end_matches(".git");
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() >= 2 {
            return Ok((parts[0].to_string(), parts[1].to_string()));
        }
    }

    anyhow::bail!("Could not parse GitHub repository from URL: {}", url)
}

/// Generate PR title from branch name