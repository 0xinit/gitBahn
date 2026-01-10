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