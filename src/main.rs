//! gitBahn - Autonomous Git operations with AI-powered commits.

use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
mod config;
mod core;

use config::Config;

#[derive(Parser)]
#[command(name = "bahn", version, about = "Autonomous Git operations with AI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate AI-powered commit messages
    Commit {
        /// Split changes into atomic commits
        #[arg(short, long)]
        atomic: bool,

        /// Use conventional commit format
        #[arg(long)]
        conventional: bool,

        /// AI personality/agent to use
        #[arg(long)]
        agent: Option<String>,

        /// Auto-confirm without prompting
        #[arg(short = 'y', long)]
        yes: bool,

        /// Spread atomic commits over time (e.g., "2h", "30m", "1d")
        #[arg(long)]
        spread: Option<String>,

        /// Start time for atomic commits (e.g., "2025-12-25 09:00")
        #[arg(long)]
        start: Option<String>,
    },

    /// Autonomous mode - watch and auto-commit
    Auto {
        /// Watch for changes continuously
        #[arg(short, long)]
        watch: bool,

        /// Interval between checks in seconds
        #[arg(short, long, default_value = "30")]
        interval: u64,

        /// Auto-merge to target branch
        #[arg(short, long)]
        merge: bool,

        /// Target branch for auto-merge
        #[arg(long, default_value = "main")]
        target: String,

        /// Maximum commits before stopping
        #[arg(long, default_value = "100")]
        max_commits: usize,

        /// Dry run - don't actually commit
        #[arg(long)]
        dry_run: bool,
    },

    /// AI-powered code rewrite
    Rewrite {
        /// Path to rewrite
        path: String,

        /// Rewrite instructions
        #[arg(short, long)]
        instructions: Option<String>,

        /// Dry run - show changes without applying
        #[arg(long)]
        dry_run: bool,
    },

    /// AI-assisted merge with conflict resolution
    Merge {
        /// Branch to merge
        branch: String,

        /// Auto-resolve conflicts with AI
        #[arg(long)]
        auto_resolve: bool,
    },

    /// Generate documentation for code
    Docs {
        /// Path to document
        path: String,

        /// Documentation format (rust, markdown, jsdoc)
        #[arg(short, long, default_value = "rust")]
        format: String,
    },

    /// AI-powered code review
    Review {
        /// Review staged changes
        #[arg(long)]
        staged: bool,

        /// Review specific commit
        #[arg(long)]
        commit: Option<String>,

        /// Strictness level (relaxed, normal, strict)
        #[arg(long, default_value = "normal")]
        strictness: String,
    },

    /// Initialize gitBahn in a repository
    Init {
        /// Path to initialize
        path: Option<String>,
    },

    /// Show repository status
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::load(None)?;

    match cli.command {
        Commands::Commit { atomic, conventional, agent, yes, spread, start } => {
            let options = commands::commit::CommitOptions {
                atomic,
                conventional,
                agent,
                auto_confirm: yes,
                verbose: cli.verbose,
                spread,
                start,
            };
            commands::commit::run(options, &config).await
        }

        Commands::Auto { watch, interval, merge, target, max_commits, dry_run } => {
            commands::auto::run(&config, watch, interval, merge, &target, max_commits, dry_run).await
        }

        Commands::Rewrite { path, instructions, dry_run } => {
            commands::rewrite::run(&config, &path, instructions.as_deref(), dry_run).await
        }

        Commands::Merge { branch, auto_resolve } => {
            commands::merge::run(&config, &branch, auto_resolve).await
        }

        Commands::Docs { path, format } => {
            commands::docs::run(&config, &path, &format).await
        }

        Commands::Review { staged, commit, strictness } => {
            commands::review::run(&config, staged, commit.as_deref(), &strictness).await
        }

        Commands::Init { path } => {
            commands::init::run(path.as_deref())
        }

        Commands::Status => {
            commands::status::run()
        }
    }
}
