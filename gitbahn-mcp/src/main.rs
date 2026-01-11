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