//! Merge command - AI-assisted merge with conflict resolution.

use anyhow::{Context, Result};
use colored::Colorize;
use git2::MergeOptions;

use crate::config::Config;
use crate::core::ai::AiClient;
use crate::core::git;

/// Run the merge command
pub async fn run(config: &Config, branch: &str, auto_resolve: bool) -> Result<()> {
    println!("{}", "gitBahn - AI Merge".bold().cyan());
    println!();

    let repo = git::open_repo(None)?;
    let current = git::current_branch(&repo)?;

    println!("Merging {} into {}", branch.yellow(), current.green());

    // Find the branch to merge
    let branch_ref = repo.find_branch(branch, git2::BranchType::Local)
        .with_context(|| format!("Branch not found: {}", branch))?;

    let branch_commit = branch_ref.get().peel_to_commit()?;
    let annotated = repo.find_annotated_commit(branch_commit.id())?;

    // Perform merge analysis
    let (analysis, _) = repo.merge_analysis(&[&annotated])?;

    if analysis.is_up_to_date() {
        println!("{}", "Already up to date.".green());
        return Ok(());
    }

    if analysis.is_fast_forward() {
        println!("{}", "Fast-forward merge possible".dimmed());
        let refname = format!("refs/heads/{}", current);
        let mut reference = repo.find_reference(&refname)?;
        reference.set_target(branch_commit.id(), "Fast-forward merge")?;
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
        println!("{} Fast-forward merge complete", "".green());
        return Ok(());
    }

    // Normal merge - may have conflicts
    let mut merge_opts = MergeOptions::new();
    repo.merge(&[&annotated], Some(&mut merge_opts), None)?;

    // Check for conflicts
    let mut index = repo.index()?;

    if index.has_conflicts() {
        println!("{}", "Merge conflicts detected!".red().bold());

        if auto_resolve {
            resolve_conflicts_with_ai(config, &repo).await?;
        } else {
            println!("Run with --auto-resolve to use AI conflict resolution");
            println!("Or resolve manually and run: git commit");
        }
    } else {
        // No conflicts - create merge commit
        let sig = repo.signature()?;
        let head = repo.head()?.peel_to_commit()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;

        let msg = format!("Merge branch '{}' into {}", branch, current);
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &msg,
            &tree,
            &[&head, &branch_commit],
        )?;

        repo.cleanup_state()?;
        println!("{} Merge complete", "".green());
    }

    Ok(())
}

async fn resolve_conflicts_with_ai(config: &Config, repo: &git2::Repository) -> Result<()> {
    let api_key = config.anthropic_api_key()
        .context("ANTHROPIC_API_KEY not set")?;

    let ai = AiClient::new(api_key.to_string(), Some(config.ai.model.clone()));
    let mut index = repo.index()?;

    let conflicts: Vec<_> = index.conflicts()?.collect();

    for conflict in conflicts {
        let conflict = conflict?;

        if let (Some(ancestor), Some(ours), Some(theirs)) = (conflict.ancestor, conflict.our, conflict.their) {
            let path = String::from_utf8_lossy(&ours.path).to_string();
            println!("  {} {}", "Resolving".yellow(), path);

            let ancestor_content = get_blob_content(repo, ancestor.id)?;
            let ours_content = get_blob_content(repo, ours.id)?;
            let theirs_content = get_blob_content(repo, theirs.id)?;

            let resolved = ai.resolve_conflict(&ancestor_content, &ours_content, &theirs_content).await?;

            // Write resolved content
            std::fs::write(&path, &resolved)?;

            // Stage the resolved file
            index.add_path(std::path::Path::new(&path))?;

            println!("  {} {}", "Resolved".green(), path);
        }
    }

    index.write()?;

    // Create merge commit
    let sig = repo.signature()?;
    let head = repo.head()?.peel_to_commit()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;

    let msg = "Merge with AI-resolved conflicts";
    repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &[&head])?;

    repo.cleanup_state()?;
    println!("{} All conflicts resolved with AI", "".green());

    Ok(())
}

fn get_blob_content(repo: &git2::Repository, oid: git2::Oid) -> Result<String> {
    let blob = repo.find_blob(oid)?;
    let content = std::str::from_utf8(blob.content())
        .context("Invalid UTF-8 in blob")?;
    Ok(content.to_string())
}
