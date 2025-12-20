//! Init command - Initialize gitBahn in a repository.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use colored::Colorize;

const DEFAULT_CONFIG: &str = r#"# gitBahn Configuration

[ai]
model = "claude-sonnet-4-20250514"

[auto]
interval = 30
max_commits = 100

[commit]
conventional = true
"#;

/// Run the init command
pub fn run(path: Option<&str>) -> Result<()> {
    println!("{}", "gitBahn - Initialize".bold().cyan());
    println!();

    let base_path = path.map(Path::new).unwrap_or(Path::new("."));
    let git_path = base_path.join(".git");
    let config_path = base_path.join(".bahn.toml");

    // Check if it's a git repository
    if !git_path.exists() {
        println!("{}", "Not a git repository. Initializing git...".yellow());
        std::process::Command::new("git")
            .arg("init")
            .current_dir(base_path)
            .output()
            .context("Failed to initialize git repository")?;
        println!("{} Initialized git repository", "".green());
    }

    // Create config file
    if config_path.exists() {
        println!("{}", "Config file already exists: .bahn.toml".yellow());
    } else {
        fs::write(&config_path, DEFAULT_CONFIG)
            .context("Failed to create config file")?;
        println!("{} Created .bahn.toml", "".green());
    }

    // Add config to .gitignore if not already
    let gitignore_path = base_path.join(".gitignore");
    let gitignore_entry = ".bahn.toml";

    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path)?;
        if !content.contains(gitignore_entry) {
            let new_content = format!("{}\n{}\n", content.trim_end(), gitignore_entry);
            fs::write(&gitignore_path, new_content)?;
            println!("{} Added .bahn.toml to .gitignore", "".green());
        }
    } else {
        fs::write(&gitignore_path, format!("{}\n", gitignore_entry))?;
        println!("{} Created .gitignore", "".green());
    }

    println!();
    println!("{}", "gitBahn initialized!".green().bold());
    println!();
    println!("Next steps:");
    println!("  1. Set ANTHROPIC_API_KEY environment variable");
    println!("  2. Run {} to generate AI commit messages", "bahn commit".cyan());
    println!("  3. Run {} for autonomous mode", "bahn auto --watch".cyan());

    Ok(())
}
