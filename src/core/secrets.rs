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
struct SecretPattern {
    name: &'static str,
    pattern: &'static str,
    confidence: f64,
}

/// All secret patterns to check
const SECRET_PATTERNS: &[SecretPattern] = &[
    // API Keys
    SecretPattern {
        name: "OpenAI API Key",
        pattern: r"sk-[a-zA-Z0-9]{20,}",
        confidence: 0.95,
    },
    SecretPattern {
        name: "Anthropic API Key",
        pattern: r"sk-ant-[a-zA-Z0-9\-]{20,}",
        confidence: 0.95,
    },
    SecretPattern {
        name: "Generic API Key",
        pattern: r#"(?i)(api[_-]?key|apikey)\s*[=:]\s*['""]?[a-zA-Z0-9\-_]{20,}['""]?"#,
        confidence: 0.8,
    },
    // AWS
    SecretPattern {
        name: "AWS Access Key ID",
        pattern: r"AKIA[0-9A-Z]{16}",
        confidence: 0.95,
    },
    SecretPattern {
        name: "AWS Secret Access Key",
        pattern: r#"(?i)(aws_secret_access_key|aws_secret)\s*[=:]\s*['""]?[A-Za-z0-9/+=]{40}['""]?"#,
        confidence: 0.9,
    },
    // GitHub
    SecretPattern {
        name: "GitHub Personal Access Token",
        pattern: r"ghp_[a-zA-Z0-9]{36}",
        confidence: 0.95,
    },
    SecretPattern {
        name: "GitHub OAuth Token",
        pattern: r"gho_[a-zA-Z0-9]{36}",
        confidence: 0.95,
    },
    SecretPattern {
        name: "GitHub App Token",
        pattern: r"ghu_[a-zA-Z0-9]{36}",
        confidence: 0.95,
    },
    SecretPattern {
        name: "GitHub Refresh Token",
        pattern: r"ghr_[a-zA-Z0-9]{36}",
        confidence: 0.95,
    },
    // GitLab
    SecretPattern {
        name: "GitLab Personal Access Token",
        pattern: r"glpat-[a-zA-Z0-9\-]{20,}",
        confidence: 0.95,
    },
    // Slack
    SecretPattern {
        name: "Slack Token",
        pattern: r"xox[baprs]-[0-9]{10,}-[0-9]{10,}-[a-zA-Z0-9]{20,}",
        confidence: 0.95,
    },
    SecretPattern {
        name: "Slack Webhook",
        pattern: r"https://hooks\.slack\.com/services/T[a-zA-Z0-9_]{8}/B[a-zA-Z0-9_]{8,}/[a-zA-Z0-9_]{24}",
        confidence: 0.95,
    },
    // Stripe
    SecretPattern {
        name: "Stripe Secret Key",
        pattern: r"sk_live_[a-zA-Z0-9]{24,}",
        confidence: 0.95,
    },
    SecretPattern {
        name: "Stripe Publishable Key",
        pattern: r"pk_live_[a-zA-Z0-9]{24,}",
        confidence: 0.7, // Lower confidence as publishable keys are meant to be public
    },
    // Private Keys
    SecretPattern {
        name: "RSA Private Key",
        pattern: r"-----BEGIN RSA PRIVATE KEY-----",
        confidence: 0.99,
    },
    SecretPattern {
        name: "OpenSSH Private Key",
        pattern: r"-----BEGIN OPENSSH PRIVATE KEY-----",
        confidence: 0.99,
    },
    SecretPattern {
        name: "EC Private Key",
        pattern: r"-----BEGIN EC PRIVATE KEY-----",
        confidence: 0.99,
    },
    SecretPattern {
        name: "PGP Private Key",
        pattern: r"-----BEGIN PGP PRIVATE KEY BLOCK-----",
        confidence: 0.99,
    },
    SecretPattern {
        name: "Generic Private Key",
        pattern: r"-----BEGIN [\w\s]+ PRIVATE KEY-----",
        confidence: 0.95,
    },
    // Database URLs
    SecretPattern {
        name: "Database Connection String",
        pattern: r#"(?i)(postgres|mysql|mongodb|redis)://[^:]+:[^@]+@[^\s'""]+"#,
        confidence: 0.9,
    },
    // Generic Secrets
    SecretPattern {
        name: "Generic Secret",
        pattern: r#"(?i)(secret|password|passwd|pwd)\s*[=:]\s*['""][^'""]{8,}['""]"#,
        confidence: 0.7,
    },
    SecretPattern {
        name: "Bearer Token",
        pattern: r#"(?i)bearer\s+[a-zA-Z0-9\-_\.]{20,}"#,
        confidence: 0.8,
    },
    SecretPattern {
        name: "JWT Token",
        pattern: r"eyJ[a-zA-Z0-9\-_]+\.eyJ[a-zA-Z0-9\-_]+\.[a-zA-Z0-9\-_]+",
        confidence: 0.85,
    },
    // Heroku
    SecretPattern {
        name: "Heroku API Key",
        pattern: r"(?i)heroku.*[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}",
        confidence: 0.8,
    },
    // Twilio
    SecretPattern {
        name: "Twilio API Key",
        pattern: r"SK[a-f0-9]{32}",
        confidence: 0.85,
    },
    // SendGrid
    SecretPattern {
        name: "SendGrid API Key",
        pattern: r"SG\.[a-zA-Z0-9\-_]{22}\.[a-zA-Z0-9\-_]{43}",
        confidence: 0.95,
    },
    // npm
    SecretPattern {
        name: "npm Token",
        pattern: r"npm_[a-zA-Z0-9]{36}",
        confidence: 0.95,
    },
    // Discord
    SecretPattern {
        name: "Discord Bot Token",
        pattern: r"[MN][a-zA-Z0-9]{23,}\.[a-zA-Z0-9\-_]{6}\.[a-zA-Z0-9\-_]{27}",
        confidence: 0.85,
    },
    SecretPattern {
        name: "Discord Webhook",
        pattern: r"https://discord(?:app)?\.com/api/webhooks/[0-9]+/[a-zA-Z0-9\-_]+",
        confidence: 0.95,
    },
];

/// Compiled regex patterns (lazily initialized)
static COMPILED_PATTERNS: Lazy<Vec<(String, Regex, f64)>> = Lazy::new(|| {
    SECRET_PATTERNS
        .iter()
        .filter_map(|p| {
            Regex::new(p.pattern)
                .ok()
                .map(|r| (p.name.to_string(), r, p.confidence))
        })
        .collect()
});

/// Detect secrets in file content
pub fn detect_secrets(content: &str, file_path: &str) -> Vec<SecretMatch> {
    // Skip binary files and common non-secret files
    if should_skip_file(file_path) {
        return Vec::new();
    }

    let mut matches = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        // Skip comments in most languages (basic heuristic)
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with("--") {
            // Still check for actual secrets in comments (they shouldn't be there either)
            // but with reduced confidence
        }

        for (name, pattern, confidence) in COMPILED_PATTERNS.iter() {
            if let Some(m) = pattern.find(line) {
                // Mask the secret value for safe display
                let matched = m.as_str();
                let masked = mask_secret(matched);

                matches.push(SecretMatch {
                    secret_type: name.clone(),
                    line: line_num + 1,
                    masked_value: masked,
                    confidence: *confidence,
                    file_path: file_path.to_string(),
                });
            }
        }
    }

    // Deduplicate matches on the same line
    matches.sort_by(|a, b| {
        a.line.cmp(&b.line)
            .then(b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal))
    });
    matches.dedup_by(|a, b| a.line == b.line && a.secret_type == b.secret_type);

    matches
}

/// Check if we should skip this file type
fn should_skip_file(file_path: &str) -> bool {
    let path_lower = file_path.to_lowercase();

    // Skip lock files
    if path_lower.ends_with(".lock") || path_lower.ends_with("-lock.json") {
        return true;
    }

    // Skip binary extensions
    let binary_extensions = [
        ".png", ".jpg", ".jpeg", ".gif", ".ico", ".svg",
        ".woff", ".woff2", ".ttf", ".eot",
        ".exe", ".dll", ".so", ".dylib",
        ".zip", ".tar", ".gz", ".rar", ".7z",
        ".pdf", ".doc", ".docx",
        ".pyc", ".pyo", ".class",
        ".o", ".a", ".lib",
    ];

    for ext in binary_extensions {
        if path_lower.ends_with(ext) {
            return true;
        }
    }

    false
}

/// Mask a secret value for safe display
fn mask_secret(secret: &str) -> String {
    let len = secret.len();
    if len <= 8 {
        "*".repeat(len)
    } else if len <= 20 {
        format!("{}...{}", &secret[..4], &secret[len-4..])
    } else {
        format!("{}...{}", &secret[..6], &secret[len-6..])
    }
}

/// Check staged changes for secrets
pub fn check_diff_for_secrets(diff: &str) -> Vec<SecretMatch> {
    let mut all_matches = Vec::new();
    let mut current_file = String::new();

    for line in diff.lines() {
        // Track current file
        if line.starts_with("diff --git") {
            let parts: Vec<&str> = line.split(' ').collect();
            if parts.len() >= 4 {
                current_file = parts[3].trim_start_matches("b/").to_string();
            }
            continue;
        }

        // Only check added lines
        if line.starts_with('+') && !line.starts_with("+++") {
            let content = &line[1..]; // Remove the + prefix
            let matches = detect_secrets(content, &current_file);
            for m in matches {
                all_matches.push(m);
            }
        }
    }

    all_matches
}

/// Format secrets for display
pub fn format_secret_warnings(secrets: &[SecretMatch]) -> String {
    if secrets.is_empty() {
        return String::new();
    }

    let mut output = String::new();
    output.push_str("\n⚠️  POTENTIAL SECRETS DETECTED:\n");
    output.push_str("─".repeat(50).as_str());
    output.push('\n');

    for secret in secrets {
        output.push_str(&format!(
            "  {} (confidence: {:.0}%)\n",
            secret.secret_type,
            secret.confidence * 100.0
        ));
        output.push_str(&format!(
            "    File: {}:{}\n",
            secret.file_path, secret.line
        ));
        output.push_str(&format!(
            "    Value: {}\n\n",
            secret.masked_value
        ));
    }

    output.push_str("─".repeat(50).as_str());
    output.push_str("\n\n");
    output.push_str("Consider using environment variables or a secrets manager.\n");
    output.push_str("Use --force to commit anyway (not recommended).\n");

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_openai_key() {
        let content = "OPENAI_API_KEY=sk-1234567890abcdefghijklmnop";
        let matches = detect_secrets(content, "config.py");
        assert!(!matches.is_empty());
    }

    #[test]
    fn test_detect_aws_key() {
        let content = "aws_access_key_id = AKIAIOSFODNN7EXAMPLE";
        let matches = detect_secrets(content, ".env");
        assert!(!matches.is_empty());
    }

    #[test]
    fn test_detect_github_token() {
        let content = "token: ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
        let matches = detect_secrets(content, "config.yml");
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|m| m.secret_type.contains("GitHub")));
    }

    #[test]
    fn test_detect_private_key() {
        let content = "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQ...";
        let matches = detect_secrets(content, "key.pem");
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|m| m.secret_type.contains("Private Key")));
    }

    #[test]
    fn test_skip_lock_files() {
        assert!(should_skip_file("Cargo.lock"));
        assert!(should_skip_file("package-lock.json"));
        assert!(should_skip_file("yarn.lock"));
    }

    #[test]
    fn test_mask_secret() {
        assert_eq!(mask_secret("short"), "*****");
        assert_eq!(mask_secret("medium-length-key"), "medi...-key");
        assert_eq!(mask_secret("this-is-a-very-long-secret-key-value"), "this-i...-value");
    }
}
