//! gitBahn MCP Server
//!
//! Thin git operations layer for Claude Code. No AI calls - Claude Code handles
//! commit message generation directly.

use std::process::Command;
use rmcp::{
    ErrorData as McpError,
    ServerHandler,
    model::*,
    tool, tool_router, tool_handler,
    handler::server::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    service::serve_server,
    transport::io::stdio,
};
use schemars::JsonSchema;
use serde::Deserialize;

/// Request for staging specific files
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StageFilesRequest {
    /// Files to stage (paths relative to repo root)
    #[schemars(description = "List of file paths to stage")]
    pub files: Vec<String>,
}

/// Request for creating a commit
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateCommitRequest {
    /// Commit message
    #[schemars(description = "The commit message")]
    pub message: String,

    /// Optional timestamp for backdating (e.g., "2025-01-03 11:17")
    #[schemars(description = "Optional timestamp for the commit (e.g., '2025-01-03 11:17')")]
    pub timestamp: Option<String>,
}

/// Request for getting diff
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetDiffRequest {
    /// Whether to get staged diff only (default: true)
    #[schemars(description = "Get staged changes only (default: true). Set to false for unstaged changes.")]
    pub staged: Option<bool>,

    /// Specific files to get diff for
    #[schemars(description = "Optional list of specific files to get diff for")]
    pub files: Option<Vec<String>>,
}

/// Request for getting commit log
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetLogRequest {
    /// Number of commits to show
    #[schemars(description = "Number of commits to show (default: 10)")]
    pub count: Option<u32>,

    /// Show full commit messages
    #[schemars(description = "Show full commit messages instead of one-line format")]
    pub full: Option<bool>,
}

/// Request for push
#[derive(Debug, Deserialize, JsonSchema)]
pub struct PushRequest {
    /// Remote name (default: origin)
    #[schemars(description = "Remote name (default: origin)")]
    pub remote: Option<String>,

    /// Branch name (default: current branch)
    #[schemars(description = "Branch name (default: current branch)")]
    pub branch: Option<String>,

    /// Force push
    #[schemars(description = "Force push (use with caution)")]
    pub force: Option<bool>,
}

/// Request for undo
#[derive(Debug, Deserialize, JsonSchema)]
pub struct UndoRequest {
    /// Number of commits to undo
    #[schemars(description = "Number of commits to undo (default: 1)")]
    pub count: Option<u32>,

    /// Hard reset (discard changes)
    #[schemars(description = "Hard reset - discard changes (default: false, keeps changes staged)")]
    pub hard: Option<bool>,
}

/// gitBahn MCP Server handler
#[derive(Clone)]
pub struct GitBahnServer {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl GitBahnServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    /// Get git status showing staged and unstaged changes.
    #[tool(description = "Get git status showing staged and unstaged changes with file status indicators")]
    async fn get_status(&self) -> Result<CallToolResult, McpError> {
        let result = run_git(&["status", "--short"]);
        let output = if result.is_empty() {
            "Working tree clean - no changes to commit.".to_string()
        } else {
            format!("Status:\n{}\n\nLegend: M=modified, A=added, D=deleted, R=renamed, ??=untracked\nFirst column=staged, second column=unstaged", result)
        };
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// Get diff of changes.
    #[tool(description = "Get diff of staged or unstaged changes. Use this to see what will be committed.")]
    async fn get_diff(&self, params: Parameters<GetDiffRequest>) -> Result<CallToolResult, McpError> {
        let req = params.0;
        let staged = req.staged.unwrap_or(true);

        let mut args = vec!["diff"];
        if staged {
            args.push("--cached");
        }

        // Add file paths if specified
        let files_str: Vec<&str>;
        if let Some(ref files) = req.files {
            args.push("--");
            files_str = files.iter().map(|s| s.as_str()).collect();
            args.extend(&files_str);
        }

        let result = run_git(&args);
        let output = if result.is_empty() {
            if staged {
                "No staged changes.".to_string()
            } else {
                "No unstaged changes.".to_string()
            }
        } else {
            result
        };
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// Stage all changes in the repository.
    #[tool(description = "Stage all changes in the repository (git add -A)")]
    async fn stage_all(&self) -> Result<CallToolResult, McpError> {
        let _ = run_git(&["add", "-A"]);
        Ok(CallToolResult::success(vec![Content::text("All changes staged.".to_string())]))
    }

    /// Stage specific files.
    #[tool(description = "Stage specific files for commit")]
    async fn stage_files(&self, params: Parameters<StageFilesRequest>) -> Result<CallToolResult, McpError> {
        let req = params.0;
        if req.files.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("No files specified.".to_string())]));
        }

        let mut args = vec!["add".to_string(), "--".to_string()];
        args.extend(req.files.clone());

        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let _ = run_git(&args_ref);

        Ok(CallToolResult::success(vec![Content::text(format!("Staged {} file(s): {}", req.files.len(), req.files.join(", ")))]))
    }

    /// Create a commit with the given message.
    #[tool(description = "Create a commit with the provided message. Optionally backdate the commit.")]
    async fn create_commit(&self, params: Parameters<CreateCommitRequest>) -> Result<CallToolResult, McpError> {
        let req = params.0;

        // Check if there are staged changes
        let staged = run_git(&["diff", "--cached", "--stat"]);
        if staged.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("Nothing to commit - no staged changes.".to_string())]));
        }

        let result = if let Some(timestamp) = req.timestamp {
            // Commit with custom timestamp
            let date_str = format!("{} +0000", timestamp);
            match Command::new("git")
                .args(["commit", "-m", &req.message])
                .env("GIT_AUTHOR_DATE", &date_str)
                .env("GIT_COMMITTER_DATE", &date_str)
                .output()
            {
                Ok(output) => {
                    if output.status.success() {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        format!("Committed with timestamp {}:\n{}", timestamp, stdout)
                    } else {
                        format!("Commit failed: {}", String::from_utf8_lossy(&output.stderr))
                    }
                }
                Err(e) => format!("Error: {}", e),
            }
        } else {
            // Normal commit
            match Command::new("git")
                .args(["commit", "-m", &req.message])
                .output()
            {
                Ok(output) => {
                    if output.status.success() {
                        String::from_utf8_lossy(&output.stdout).to_string()
                    } else {
                        format!("Commit failed: {}", String::from_utf8_lossy(&output.stderr))
                    }
                }
                Err(e) => format!("Error: {}", e),
            }
        };

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Get recent commit history.
    #[tool(description = "Get recent commit history with timestamps and messages")]
    async fn get_log(&self, params: Parameters<GetLogRequest>) -> Result<CallToolResult, McpError> {
        let req = params.0;
        let count = req.count.unwrap_or(10).to_string();

        let format = if req.full.unwrap_or(false) {
            "%h %ci%n  %s%n  %b"
        } else {
            "%h %ci %s"
        };

        let result = run_git(&["log", &format!("-{}", count), &format!("--format={}", format)]);
        let output = if result.is_empty() {
            "No commits yet.".to_string()
        } else {
            result
        };
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// Get current branch name.
    #[tool(description = "Get the current branch name")]
    async fn get_branch(&self) -> Result<CallToolResult, McpError> {
        let result = run_git(&["branch", "--show-current"]);
        Ok(CallToolResult::success(vec![Content::text(format!("Current branch: {}", result.trim()))]))
    }

    /// Push commits to remote.
    #[tool(description = "Push commits to remote repository")]
    async fn push(&self, params: Parameters<PushRequest>) -> Result<CallToolResult, McpError> {
        let req = params.0;
        let remote = req.remote.unwrap_or_else(|| "origin".to_string());

        let mut args = vec!["push".to_string(), remote.clone()];

        if let Some(branch) = req.branch {
            args.push(branch);
        }

        if req.force.unwrap_or(false) {
            args.insert(1, "--force-with-lease".to_string());
        }

        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let result = run_git(&args_ref);

        let output = if result.is_empty() {
            format!("Pushed to {}", remote)
        } else {
            result
        };
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// Undo recent commits.
    #[tool(description = "Undo recent commits. By default keeps changes staged (soft reset).")]
    async fn undo(&self, params: Parameters<UndoRequest>) -> Result<CallToolResult, McpError> {
        let req = params.0;
        let count = req.count.unwrap_or(1);
        let reset_type = if req.hard.unwrap_or(false) { "--hard" } else { "--soft" };

        let result = run_git(&["reset", reset_type, &format!("HEAD~{}", count)]);

        let output = format!(
            "Reset {} commit(s) with {} reset.\n{}",
            count,
            if req.hard.unwrap_or(false) { "hard (changes discarded)" } else { "soft (changes kept staged)" },
            result
        );
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// List changed files grouped by type.
    #[tool(description = "List changed files grouped by status (staged/unstaged/untracked)")]
    async fn list_changes(&self) -> Result<CallToolResult, McpError> {
        let status = run_git(&["status", "--porcelain"]);

        if status.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("No changes.".to_string())]));
        }

        let mut staged = Vec::new();
        let mut unstaged = Vec::new();
        let mut untracked = Vec::new();

        for line in status.lines() {
            if line.len() < 3 {
                continue;
            }
            let index_status = line.chars().next().unwrap_or(' ');
            let worktree_status = line.chars().nth(1).unwrap_or(' ');
            let file = &line[3..];

            if index_status == '?' {
                untracked.push(file.to_string());
            } else {
                if index_status != ' ' {
                    staged.push(format!("{} {}", index_status, file));
                }
                if worktree_status != ' ' {
                    unstaged.push(format!("{} {}", worktree_status, file));
                }
            }
        }

        let mut output = String::new();

        if !staged.is_empty() {
            output.push_str(&format!("Staged ({}):\n", staged.len()));
            for f in &staged {
                output.push_str(&format!("  {}\n", f));
            }
        }

        if !unstaged.is_empty() {
            output.push_str(&format!("\nUnstaged ({}):\n", unstaged.len()));
            for f in &unstaged {
                output.push_str(&format!("  {}\n", f));
            }
        }

        if !untracked.is_empty() {
            output.push_str(&format!("\nUntracked ({}):\n", untracked.len()));
            for f in &untracked {
                output.push_str(&format!("  {}\n", f));
            }
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }
}

/// Run a git command and return output
fn run_git(args: &[&str]) -> String {
    match Command::new("git").args(args).output() {
        Ok(output) => {
            if output.status.success() {
                String::from_utf8_lossy(&output.stdout).to_string()
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.is_empty() {
                    String::from_utf8_lossy(&output.stdout).to_string()
                } else {
                    format!("Error: {}", stderr)
                }
            }
        }
        Err(e) => format!("Failed to run git: {}", e),
    }
}

#[tool_handler]
impl ServerHandler for GitBahnServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability::default()),
                ..Default::default()
            },
            server_info: Implementation {
                name: "gitbahn-mcp".to_string(),
                title: Some("gitBahn MCP Server".to_string()),
                version: "0.2.0".to_string(),
                icons: None,
                website_url: Some("https://github.com/0xinit/gitBahn".to_string()),
            },
            instructions: Some(
                "gitBahn provides git operations for Claude Code. YOU generate commit messages \
                by analyzing diffs - no API key needed. Use get_diff to see changes, then \
                create_commit with your generated message. For realistic history, create \
                multiple small commits with timestamps spread over time.".to_string()
            ),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let server = GitBahnServer::new();
    let transport = stdio();
    serve_server(server, transport).await?;
    Ok(())
}
