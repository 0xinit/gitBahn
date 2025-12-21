//! Rewrite command - AI-powered code transformation.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use colored::Colorize;

use crate::config::Config;
use crate::core::ai::AiClient;

/// Run the rewrite command
pub async fn run(config: &Config, path: &str, instructions: Option<&str>, dry_run: bool) -> Result<()> {
    println!("{}", "gitBahn - Code Rewrite".bold().cyan());
    println!();

    let api_key = config.anthropic_api_key()
        .context("ANTHROPIC_API_KEY not set")?;

    let ai = AiClient::new(api_key.to_string(), Some(config.ai.model.clone()));

    let file_path = Path::new(path);

    if !file_path.exists() {
        anyhow::bail!("Path does not exist: {}", path);
    }

    if file_path.is_file() {
        rewrite_file(&ai, file_path, instructions, dry_run).await?;
    } else if file_path.is_dir() {
        rewrite_directory(&ai, file_path, instructions, dry_run).await?;
    }

    Ok(())
}

async fn rewrite_file(ai: &AiClient, path: &Path, instructions: Option<&str>, dry_run: bool) -> Result<()> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let extension = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("txt");

    let language = match extension {
        "rs" => "rust",
        "py" => "python",
        "js" => "javascript",
        "ts" => "typescript",
        "go" => "go",
        "rb" => "ruby",
        _ => extension,
    };

    println!("  {} {}", "Rewriting".yellow(), path.display());

    let instructions = instructions.unwrap_or("Improve code quality, fix bugs, and optimize");

    let rewritten = ai.rewrite_code(&content, language, instructions).await?;

    if dry_run {
        println!("{}", "--- Original ---".dimmed());
        println!("{}", &content[..content.len().min(500)]);
        println!("{}", "--- Rewritten ---".dimmed());
        println!("{}", &rewritten[..rewritten.len().min(500)]);
        println!("{}", "[DRY RUN] Changes not applied".yellow());
    } else {
        fs::write(path, &rewritten)
            .with_context(|| format!("Failed to write file: {}", path.display()))?;
        println!("  {} {}", "Rewrote".green(), path.display());
    }

    Ok(())
}

async fn rewrite_directory(ai: &AiClient, path: &Path, instructions: Option<&str>, dry_run: bool) -> Result<()> {
    let extensions = ["rs", "py", "js", "ts", "go", "rb"];

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();

        if entry_path.is_file() {
            if let Some(ext) = entry_path.extension().and_then(|e| e.to_str()) {
                if extensions.contains(&ext) {
                    rewrite_file(ai, &entry_path, instructions, dry_run).await?;
                }
            }
        } else if entry_path.is_dir() {
            let dir_name = entry_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !dir_name.starts_with('.') && dir_name != "target" && dir_name != "node_modules" {
                Box::pin(rewrite_directory(ai, &entry_path, instructions, dry_run)).await?;
            }
        }
    }

    Ok(())
}
