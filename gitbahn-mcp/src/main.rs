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