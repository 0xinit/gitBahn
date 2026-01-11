//! gitBahn MCP Server
//!
//! Exposes gitBahn's commit tools to AI assistants via Model Context Protocol.

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

/// Request for realistic commit mode
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RealisticCommitRequest {
    /// Target number of commits to create
    #[schemars(description = "Number of commits to split changes into (e.g., 30)")]
    pub split: Option<u32>,

    /// Time duration to spread commits over (e.g., "24h", "48h", "7d")
    #[schemars(description = "Duration to spread commits over (e.g., '24h', '48h', '7d')")]
    pub spread: Option<String>,

    /// Start time for commits (e.g., "2025-01-03 11:17")
    #[schemars(description = "Start timestamp for commits (e.g., '2025-01-03 11:17')")]
    pub start: Option<String>,

    /// Auto-confirm without prompting
    #[schemars(description = "Auto-confirm without prompting (default: true)")]
    pub auto_confirm: Option<bool>,
}

/// Request for atomic commit mode
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AtomicCommitRequest {
    /// Target number of commits
    #[schemars(description = "Number of commits to split changes into")]
    pub split: Option<u32>,

    /// Time duration to spread commits over
    #[schemars(description = "Duration to spread commits over (e.g., '4h', '24h')")]
    pub spread: Option<String>,

    /// Start time for commits
    #[schemars(description = "Start timestamp for commits (e.g., '2025-01-03 11:17')")]
    pub start: Option<String>,

    /// Auto-confirm without prompting
    #[schemars(description = "Auto-confirm without prompting (default: true)")]
    pub auto_confirm: Option<bool>,
}

/// Request for granular commit mode
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GranularCommitRequest {
    /// Target number of commits
    #[schemars(description = "Number of commits to split changes into")]
    pub split: Option<u32>,

    /// Time duration to spread commits over
    #[schemars(description = "Duration to spread commits over (e.g., '4h', '24h')")]
    pub spread: Option<String>,

    /// Start time for commits
    #[schemars(description = "Start timestamp for commits (e.g., '2025-01-03 11:17')")]
    pub start: Option<String>,

    /// Auto-confirm without prompting
    #[schemars(description = "Auto-confirm without prompting (default: true)")]
    pub auto_confirm: Option<bool>,
}

/// Request for simple AI commit
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SimpleCommitRequest {
    /// Auto-confirm without prompting
    #[schemars(description = "Auto-confirm without prompting (default: true)")]
    pub auto_confirm: Option<bool>,
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

    /// Create commits with realistic human-like development flow.
    #[tool(description = "Create realistic commits that simulate human development flow. Splits files into logical chunks (imports, classes, methods) and commits them progressively over time. Best for new projects.")]
    async fn realistic_commit(&self, params: Parameters<RealisticCommitRequest>) -> Result<CallToolResult, McpError> {
        let req = params.0;
        let mut args = vec!["commit".to_string(), "--realistic".to_string()];

        if let Some(split) = req.split {
            args.push("--split".to_string());
            args.push(split.to_string());
        }

        if let Some(spread) = req.spread {
            args.push("--spread".to_string());
            args.push(spread);
        }

        if let Some(start) = req.start {
            args.push("--start".to_string());
            args.push(start);
        }

        if req.auto_confirm.unwrap_or(true) {
            args.push("-y".to_string());
        }

        let result = run_bahn_command(&args);
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Create atomic commits by splitting changes by file.
    #[tool(description = "Create atomic commits split by file. Each changed file gets its own commit. Good for quick splitting of changes.")]
    async fn atomic_commit(&self, params: Parameters<AtomicCommitRequest>) -> Result<CallToolResult, McpError> {
        let req = params.0;
        let mut args = vec!["commit".to_string(), "--atomic".to_string()];

        if let Some(split) = req.split {
            args.push("--split".to_string());
            args.push(split.to_string());
        }

        if let Some(spread) = req.spread {
            args.push("--spread".to_string());
            args.push(spread);
        }

        if let Some(start) = req.start {
            args.push("--start".to_string());
            args.push(start);
        }

        if req.auto_confirm.unwrap_or(true) {
            args.push("-y".to_string());
        }

        let result = run_bahn_command(&args);
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Create granular commits by splitting changes by hunks (diff chunks).
    #[tool(description = "Create granular commits split by hunks (diff chunks within files). Allows splitting a single file across multiple commits. Best for modified files.")]
    async fn granular_commit(&self, params: Parameters<GranularCommitRequest>) -> Result<CallToolResult, McpError> {
        let req = params.0;
        let mut args = vec!["commit".to_string(), "--granular".to_string()];

        if let Some(split) = req.split {
            args.push("--split".to_string());
            args.push(split.to_string());
        }

        if let Some(spread) = req.spread {
            args.push("--spread".to_string());
            args.push(spread);
        }

        if let Some(start) = req.start {
            args.push("--start".to_string());
            args.push(start);
        }

        if req.auto_confirm.unwrap_or(true) {
            args.push("-y".to_string());
        }

        let result = run_bahn_command(&args);
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Create a single commit with AI-generated message.
    #[tool(description = "Create a single commit with AI-generated message for all staged changes.")]
    async fn simple_commit(&self, params: Parameters<SimpleCommitRequest>) -> Result<CallToolResult, McpError> {
        let req = params.0;
        let mut args = vec!["commit".to_string()];

        if req.auto_confirm.unwrap_or(true) {
            args.push("-y".to_string());
        }

        let result = run_bahn_command(&args);
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Stage all changes in the repository.
    #[tool(description = "Stage all changes in the repository (git add -A)")]
    async fn stage_all(&self) -> Result<CallToolResult, McpError> {
        let result = match Command::new("git")
            .args(["add", "-A"])
            .output()
        {
            Ok(output) => {
                if output.status.success() {
                    "All changes staged successfully.".to_string()
                } else {
                    format!("Failed to stage: {}", String::from_utf8_lossy(&output.stderr))
                }
            }
            Err(e) => format!("Error running git: {}", e),
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Get git status showing staged and unstaged changes.
    #[tool(description = "Get git status showing staged and unstaged changes")]
    async fn git_status(&self) -> Result<CallToolResult, McpError> {
        let result = match Command::new("git")
            .args(["status", "--short"])
            .output()
        {
            Ok(output) => {
                if output.status.success() {
                    let status = String::from_utf8_lossy(&output.stdout);
                    if status.is_empty() {
                        "Working tree clean - no changes to commit.".to_string()
                    } else {
                        format!("Changes:\n{}", status)
                    }
                } else {
                    format!("Failed: {}", String::from_utf8_lossy(&output.stderr))
                }
            }
            Err(e) => format!("Error running git: {}", e),
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }
}

/// Run a bahn CLI command and return output