//! Configuration management for gitBahn.

use std::path::PathBuf;
use std::fs;

use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};

/// Default configuration file name
const CONFIG_FILE: &str = ".bahn.toml";

/// Global configuration directory
fn global_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("gitBahn")
}

/// Configuration for gitBahn
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// AI provider settings
    #[serde(default)]
    pub ai: AiConfig,

    /// Commit settings
    #[serde(default)]
    pub commit: CommitConfig,

    /// Auto mode settings
    #[serde(default)]
    pub auto: AutoConfig,

    /// Documentation settings
    #[serde(default)]
    pub docs: DocsConfig,

    /// Review settings
    #[serde(default)]
    pub review: ReviewConfig,

    /// GitHub settings
    #[serde(default)]
    pub github: GitHubConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    /// Anthropic API key (can also use ANTHROPIC_API_KEY env var)
    #[serde(default)]
    pub anthropic_api_key: Option<String>,

    /// OpenAI API key for embeddings (can also use OPENAI_API_KEY env var)
    #[serde(default)]
    pub openai_api_key: Option<String>,

    /// Default model to use
    #[serde(default = "default_model")]
    pub model: String,

    /// Elite Coder API URL (for personality agents)
    #[serde(default)]
    pub elite_coder_url: Option<String>,
}

fn default_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            anthropic_api_key: None,
            openai_api_key: None,
            model: default_model(),
            elite_coder_url: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitConfig {
    /// Use conventional commits format
    #[serde(default = "default_true")]
    pub conventional: bool,

    /// Default to atomic commits
    #[serde(default)]
    pub atomic: bool,

    /// Sign commits with GPG
    #[serde(default)]
    pub sign: bool,

    /// Default personality agent for commits
    #[serde(default)]
    pub default_agent: Option<String>,

    /// Commit message template
    #[serde(default)]
    pub template: Option<String>,
}

fn default_true() -> bool {
    true
}

impl Default for CommitConfig {
    fn default() -> Self {
        Self {
            conventional: true,
            atomic: false,
            sign: false,
            default_agent: None,
            template: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoConfig {
    /// Watch interval in seconds (0 for event-based)
    #[serde(default = "default_interval")]
    pub interval: u64,

    /// Maximum commits before stopping
    #[serde(default = "default_max_commits")]
    pub max_commits: usize,

    /// Enable history rewriting (squash commits)
    #[serde(default)]
    pub rewrite_history: bool,

    /// Number of commits before auto-squash triggers
    #[serde(default = "default_squash_threshold")]
    pub squash_threshold: usize,

    /// Auto-push after squash
    #[serde(default)]
    pub auto_push: bool,
}

fn default_interval() -> u64 {
    30
}

fn default_max_commits() -> usize {
    100
}

fn default_squash_threshold() -> usize {
    5
}

impl Default for AutoConfig {
    fn default() -> Self {
        Self {
            interval: default_interval(),
            max_commits: default_max_commits(),
            rewrite_history: false,
            squash_threshold: default_squash_threshold(),
            auto_push: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocsConfig {
    /// Default documentation format
    #[serde(default = "default_doc_format")]
    pub format: String,

    /// Files/patterns to exclude
    #[serde(default)]
    pub exclude: Vec<String>,

    /// Update existing docs or only add new
    #[serde(default)]
    pub update_existing: bool,
}

fn default_doc_format() -> String {
    "auto".to_string()
}

impl Default for DocsConfig {
    fn default() -> Self {
        Self {
            format: default_doc_format(),
            exclude: vec![
                "node_modules".to_string(),
                "target".to_string(),
                ".git".to_string(),
                "vendor".to_string(),
            ],
            update_existing: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewConfig {
    /// Default personality agent for reviews
    #[serde(default)]
    pub default_agent: Option<String>,

    /// Automatically post reviews to GitHub
    #[serde(default)]
    pub auto_post: bool,

    /// Review strictness level (relaxed, normal, strict)
    #[serde(default = "default_strictness")]
    pub strictness: String,
}

fn default_strictness() -> String {
    "normal".to_string()
}

impl Default for ReviewConfig {
    fn default() -> Self {
        Self {
            default_agent: None,
            auto_post: false,
            strictness: default_strictness(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitHubConfig {
    /// GitHub token (can also use GITHUB_TOKEN env var)
    #[serde(default)]
    pub token: Option<String>,

    /// Default repository (owner/repo)
    #[serde(default)]
    pub default_repo: Option<String>,
}

impl Config {
    /// Load configuration from file(s)
    pub fn load(path: Option<&str>) -> Result<Self> {
        // Priority: explicit path > project config > global config > defaults
        let config = if let Some(path) = path {
            let content = fs::read_to_string(path)
                .with_context(|| format!("Failed to read config file: {}", path))?;
            toml::from_str(&content)?
        } else {
            // Try project-local config first
            let local_path = PathBuf::from(CONFIG_FILE);
            if local_path.exists() {
                let content = fs::read_to_string(&local_path)?;
                toml::from_str(&content)?
            } else {
                // Try global config
                let global_path = global_config_dir().join("config.toml");
                if global_path.exists() {
                    let content = fs::read_to_string(&global_path)?;
                    toml::from_str(&content)?
                } else {
                    Config::default()
                }
            }
        };

        // Override with environment variables
        Ok(config.with_env_overrides())
    }

    /// Apply environment variable overrides
    fn with_env_overrides(mut self) -> Self {
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            self.ai.anthropic_api_key = Some(key);
        }

        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            self.ai.openai_api_key = Some(key);
        }

        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            self.github.token = Some(token);
        }

        if let Ok(url) = std::env::var("ELITE_CODER_URL") {
            self.ai.elite_coder_url = Some(url);
        }

        self
    }

    /// Get the Anthropic API key
    pub fn anthropic_api_key(&self) -> Option<&str> {
        self.ai.anthropic_api_key.as_deref()
    }

    /// Get the GitHub token
    #[allow(dead_code)] // Will be used when GitHub integration is implemented
    pub fn github_token(&self) -> Option<&str> {
        self.github.token.as_deref()
    }
}

/// Initialize configuration file
#[allow(dead_code)] // Available for future CLI subcommand
pub fn init_config(force: bool) -> Result<()> {
    let local_path = PathBuf::from(CONFIG_FILE);

    if local_path.exists() && !force {
        println!(
            "{} Configuration file already exists. Use --force to overwrite.",
            "Warning:".yellow()
        );
        return Ok(());
    }

    let default_config = Config::default();
    let content = toml::to_string_pretty(&default_config)?;

    fs::write(&local_path, &content)?;

    println!("{} Created {}", "Success:".green(), CONFIG_FILE);
    println!("\nEdit the file to customize gitBahn settings.");
    println!("You can also set environment variables:");
    println!("  - ANTHROPIC_API_KEY");
    println!("  - OPENAI_API_KEY");
    println!("  - GITHUB_TOKEN");

    Ok(())
}

/// Show current configuration
#[allow(dead_code)] // Available for future CLI subcommand
pub fn show_config(config: &Config) -> Result<()> {
    println!("{}", "Current Configuration:".bold());
    println!();

    // AI settings
    println!("{}:", "AI Settings".cyan());
    println!(
        "  Model: {}",
        config.ai.model
    );
    println!(
        "  Anthropic API Key: {}",
        if config.ai.anthropic_api_key.is_some() {
            "✓ Set".green().to_string()
        } else {
            "✗ Not set".red().to_string()
        }
    );
    println!(
        "  OpenAI API Key: {}",
        if config.ai.openai_api_key.is_some() {
            "✓ Set".green().to_string()
        } else {
            "✗ Not set".red().to_string()
        }
    );

    // Commit settings
    println!("\n{}:", "Commit Settings".cyan());
    println!("  Conventional: {}", config.commit.conventional);
    println!("  Atomic: {}", config.commit.atomic);
    println!("  Sign: {}", config.commit.sign);
    if let Some(agent) = &config.commit.default_agent {
        println!("  Default Agent: {}", agent);
    }

    // Docs settings
    println!("\n{}:", "Docs Settings".cyan());
    println!("  Format: {}", config.docs.format);
    println!("  Update Existing: {}", config.docs.update_existing);

    // Review settings
    println!("\n{}:", "Review Settings".cyan());
    println!("  Strictness: {}", config.review.strictness);
    println!("  Auto Post: {}", config.review.auto_post);

    // GitHub settings
    println!("\n{}:", "GitHub Settings".cyan());
    println!(
        "  Token: {}",
        if config.github.token.is_some() {
            "✓ Set".green().to_string()
        } else {
            "✗ Not set".red().to_string()
        }
    );
    if let Some(repo) = &config.github.default_repo {
        println!("  Default Repo: {}", repo);
    }

    Ok(())
}
