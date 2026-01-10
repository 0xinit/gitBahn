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