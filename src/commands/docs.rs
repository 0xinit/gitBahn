//! Docs command - AI-powered documentation generation.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use colored::Colorize;

use crate::config::Config;
use crate::core::ai::AiClient;

/// Run the docs command
pub async fn run(config: &Config, path: &str, format: &str) -> Result<()> {
    println!("{}", "gitBahn - Documentation Generator".bold().cyan());
    println!();

    let api_key = config.anthropic_api_key()
        .context("ANTHROPIC_API_KEY not set")?;

    let ai = AiClient::new(api_key.to_string(), Some(config.ai.model.clone()));

    let file_path = Path::new(path);

    if !file_path.exists() {
        anyhow::bail!("Path does not exist: {}", path);
    }

    if file_path.is_file() {
        generate_docs_for_file(&ai, file_path, format).await?;
    } else if file_path.is_dir() {
        generate_docs_for_directory(&ai, file_path, format).await?;
    }

    Ok(())
}

async fn generate_docs_for_file(ai: &AiClient, path: &Path, format: &str) -> Result<()> {
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

    println!("  {} {}", "Documenting".yellow(), path.display());

    let docs = ai.generate_docs(&content, language, format).await?;

    println!("{}", "Generated documentation:".bold());
    println!("{}", "-".repeat(50).dimmed());
    println!("{}", docs);
    println!("{}", "-".repeat(50).dimmed());

    Ok(())
}

async fn generate_docs_for_directory(ai: &AiClient, path: &Path, format: &str) -> Result<()> {
    let extensions = ["rs", "py", "js", "ts", "go", "rb"];

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();

        if entry_path.is_file() {
            if let Some(ext) = entry_path.extension().and_then(|e| e.to_str()) {
                if extensions.contains(&ext) {
                    generate_docs_for_file(ai, &entry_path, format).await?;
                }
            }
        }
    }

    Ok(())
}
