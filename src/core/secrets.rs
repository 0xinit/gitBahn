//! Secret detection to prevent accidental credential commits.

use once_cell::sync::Lazy;
use regex::Regex;

/// A detected secret in the code
#[derive(Debug, Clone)]
pub struct SecretMatch {
    /// Type of secret detected
    pub secret_type: String,
    /// Line number (1-indexed)
    pub line: usize,
    /// The matched pattern (masked for safety)
    pub masked_value: String,
    /// Confidence level (0.0 - 1.0)
    pub confidence: f64,
    /// The file path
    pub file_path: String,
}

/// Pattern definition for secret detection