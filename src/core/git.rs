//! Git operations using libgit2.

use std::path::Path;

use anyhow::{Context, Result};
use git2::{DiffOptions, IndexAddOption, Repository, StatusOptions};

/// Information about staged changes
#[derive(Debug, Clone)]
pub struct StagedChanges {
    /// Files that were added
    pub added: Vec<String>,
    /// Files that were modified
    pub modified: Vec<String>,
    /// Files that were deleted
    pub deleted: Vec<String>,
    /// Files that were renamed (old_path, new_path)
    pub renamed: Vec<(String, String)>,
    /// Full diff as a string
    pub diff: String,
    /// Summary statistics
    pub stats: DiffStats,
}

#[derive(Debug, Clone, Default)]
pub struct DiffStats {
    #[allow(dead_code)] // Available for detailed stats display
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
}

impl StagedChanges {
    /// Check if there are any staged changes
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.modified.is_empty()
            && self.deleted.is_empty()
            && self.renamed.is_empty()
    }

    /// Get all files that changed
    pub fn all_files(&self) -> Vec<&str> {
        let mut files = Vec::new();
        files.extend(self.added.iter().map(|s| s.as_str()));
        files.extend(self.modified.iter().map(|s| s.as_str()));
        files.extend(self.deleted.iter().map(|s| s.as_str()));
        files.extend(self.renamed.iter().map(|(_, new)| new.as_str()));
        files
    }

    /// Get a summary of changes
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();

        if !self.added.is_empty() {
            parts.push(format!("{} added", self.added.len()));
        }
        if !self.modified.is_empty() {
            parts.push(format!("{} modified", self.modified.len()));
        }
        if !self.deleted.is_empty() {
            parts.push(format!("{} deleted", self.deleted.len()));
        }
        if !self.renamed.is_empty() {
            parts.push(format!("{} renamed", self.renamed.len()));
        }

        if parts.is_empty() {
            "No changes".to_string()
        } else {
            parts.join(", ")
        }
    }
}

/// Open a git repository
pub fn open_repo(path: Option<&Path>) -> Result<Repository> {
    let path = path.unwrap_or_else(|| Path::new("."));

    Repository::discover(path)
        .with_context(|| format!("Not a git repository: {}", path.display()))
}

/// Get staged changes from the repository
pub fn get_staged_changes(repo: &Repository) -> Result<StagedChanges> {
    let mut changes = StagedChanges {
        added: Vec::new(),
        modified: Vec::new(),
        deleted: Vec::new(),
        renamed: Vec::new(),
        diff: String::new(),
        stats: DiffStats::default(),
    };

    // Get the HEAD tree (or empty tree for initial commit)
    let head_tree = match repo.head() {
        Ok(head) => {
            let commit = head.peel_to_commit()?;
            Some(commit.tree()?)
        }
        Err(_) => None, // No commits yet
    };

    // Get the index (staging area)
    let index = repo.index()?;

    // Create diff between HEAD and index
    let mut diff_opts = DiffOptions::new();
    diff_opts.include_untracked(false);

    let diff = repo.diff_tree_to_index(
        head_tree.as_ref(),
        Some(&index),
        Some(&mut diff_opts),
    )?;

    // Collect file changes
    diff.foreach(
        &mut |delta, _| {
            let old_path = delta.old_file().path().map(|p| p.to_string_lossy().to_string());
            let new_path = delta.new_file().path().map(|p| p.to_string_lossy().to_string());

            match delta.status() {
                git2::Delta::Added => {
                    if let Some(path) = new_path {
                        changes.added.push(path);
                    }
                }
                git2::Delta::Modified => {
                    if let Some(path) = new_path {
                        changes.modified.push(path);
                    }
                }
                git2::Delta::Deleted => {
                    if let Some(path) = old_path {
                        changes.deleted.push(path);
                    }
                }
                git2::Delta::Renamed => {
                    if let (Some(old), Some(new)) = (old_path, new_path) {
                        changes.renamed.push((old, new));
                    }
                }
                _ => {}
            }

            true
        },
        None,
        None,
        None,
    )?;

    // Get diff stats
    let stats = diff.stats()?;
    changes.stats = DiffStats {
        files_changed: stats.files_changed(),
        insertions: stats.insertions(),
        deletions: stats.deletions(),
    };

    // Get full diff text
    let mut diff_text = String::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        let prefix = match line.origin() {
            '+' => "+",
            '-' => "-",
            ' ' => " ",
            _ => "",
        };
        if !prefix.is_empty() {
            diff_text.push_str(prefix);
        }
        if let Ok(content) = std::str::from_utf8(line.content()) {
            diff_text.push_str(content);
        }
        true
    })?;

    changes.diff = diff_text;

    Ok(changes)
}

/// Create a commit with the staged changes
pub fn create_commit(repo: &Repository, message: &str, sign: bool) -> Result<git2::Oid> {
    let signature = repo.signature()?;
    let mut index = repo.index()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;

    // Get parent commit(s)
    let parents = match repo.head() {
        Ok(head) => {
            let commit = head.peel_to_commit()?;
            vec![commit]
        }
        Err(_) => vec![], // Initial commit
    };

    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();

    let commit_id = if sign {
        // GPG signing would require additional setup
        // For now, create a regular commit
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parent_refs,
        )?
    } else {
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parent_refs,
        )?
    };

    Ok(commit_id)
}

/// Stage specific files (add to index)
pub fn stage_files(repo: &Repository, files: &[&str]) -> Result<()> {
    let mut index = repo.index()?;

    for file in files {
        let path = Path::new(file);

        // Check if file exists (for adds/modifications) or was deleted
        let workdir = repo.workdir().context("Not a working directory")?;
        let full_path = workdir.join(path);

        if full_path.exists() {
            index.add_path(path)?;
        } else {
            // File was deleted, remove from index
            index.remove_path(path)?;
        }
    }

    index.write()?;
    Ok(())
}

/// Reset the staging area (unstage all files)
pub fn reset_index(repo: &Repository) -> Result<()> {
    let head = repo.head()?.peel_to_commit()?;
    repo.reset(head.as_object(), git2::ResetType::Mixed, None)?;
    Ok(())
}

/// Stage all changes (like git add -A)
pub fn stage_all(repo: &Repository) -> Result<()> {
    let mut index = repo.index()?;
    index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
    index.write()?;
    Ok(())
}

/// Get recent commit messages for context
pub fn get_recent_commits(repo: &Repository, count: usize) -> Result<Vec<String>> {
    let mut messages = Vec::new();

    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;

    for oid in revwalk.take(count) {
        let oid = oid?;
        let commit = repo.find_commit(oid)?;
        if let Some(msg) = commit.message() {
            messages.push(msg.lines().next().unwrap_or("").to_string());
        }
    }

    Ok(messages)
}

/// Check if there are uncommitted changes
pub fn has_uncommitted_changes(repo: &Repository) -> Result<bool> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true);

    let statuses = repo.statuses(Some(&mut opts))?;

    Ok(!statuses.is_empty())
}

/// Get the current branch name
pub fn current_branch(repo: &Repository) -> Result<String> {
    let head = repo.head()?;

    if head.is_branch() {
        Ok(head
            .shorthand()
            .unwrap_or("HEAD")
            .to_string())
    } else {
        // Detached HEAD
        let oid = head.target().context("Could not get HEAD target")?;
        Ok(format!("HEAD detached at {}", &oid.to_string()[..7]))
    }
}

/// Get repository root path
pub fn repo_root(repo: &Repository) -> Result<&Path> {
    repo.workdir()
        .context("Could not get repository root (bare repository?)")
}

/// Check if commits have been pushed to remote
#[allow(dead_code)]
pub fn has_unpushed_commits(repo: &Repository) -> Result<bool> {
    let head = repo.head()?;
    let head_oid = head.target().context("Could not get HEAD target")?;

    // Try to find upstream branch
    if let Ok(branch) = repo.find_branch(
        head.shorthand().unwrap_or("HEAD"),
        git2::BranchType::Local,
    ) {
        if let Ok(upstream) = branch.upstream() {
            let upstream_oid = upstream.get().target().context("Could not get upstream target")?;
            return Ok(head_oid != upstream_oid);
        }
    }

    // No upstream, all commits are unpushed
    Ok(true)
}

/// Count unpushed commits
pub fn count_unpushed_commits(repo: &Repository) -> Result<usize> {
    let head = repo.head()?;

    // Try to find upstream branch
    if let Ok(branch) = repo.find_branch(
        head.shorthand().unwrap_or("HEAD"),
        git2::BranchType::Local,
    ) {
        if let Ok(upstream) = branch.upstream() {
            let upstream_oid = upstream.get().target().context("Could not get upstream target")?;
            let head_oid = head.target().context("Could not get HEAD target")?;

            let mut revwalk = repo.revwalk()?;
            revwalk.push(head_oid)?;
            revwalk.hide(upstream_oid)?;

            return Ok(revwalk.count());
        }
    }

    // No upstream, count all commits
    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;
    Ok(revwalk.count())
}

/// Squash the last N commits into one with a new message
pub fn squash_commits(repo: &Repository, count: usize, message: &str) -> Result<git2::Oid> {
    if count < 2 {
        anyhow::bail!("Need at least 2 commits to squash");
    }

    let signature = repo.signature()?;
    let head = repo.head()?;
    let head_commit = head.peel_to_commit()?;

    // Get the parent commit that will be the new parent after squash
    let mut current = head_commit.clone();
    for _ in 0..(count - 1) {
        current = current.parent(0).context("Not enough commits to squash")?;
    }
    let base_parent = current.parent(0).context("Cannot squash initial commits")?;

    // Get the tree from HEAD (final state after all commits)
    let tree = head_commit.tree()?;

    // Create new commit with squashed message
    let commit_id = repo.commit(
        None, // Don't update HEAD yet
        &signature,
        &signature,
        message,
        &tree,
        &[&base_parent],
    )?;

    // Update HEAD to point to the new commit
    repo.reference(
        "HEAD",
        commit_id,
        true,
        &format!("squash: {} commits", count),
    )?;

    // Reset the index to match the new HEAD
    let new_commit = repo.find_commit(commit_id)?;
    repo.reset(new_commit.as_object(), git2::ResetType::Soft, None)?;

    Ok(commit_id)
}

/// Amend the last commit with a new message
#[allow(dead_code)]
pub fn amend_last_commit(repo: &Repository, new_message: &str) -> Result<git2::Oid> {
    let head = repo.head()?;
    let head_commit = head.peel_to_commit()?;

    let commit_id = head_commit.amend(
        Some("HEAD"),
        None, // Keep author
        None, // Keep committer
        None, // Keep encoding
        Some(new_message),
        None, // Keep tree
    )?;

    Ok(commit_id)
}

/// Get commit messages for the last N commits (for squash summary)
pub fn get_commit_messages_for_squash(repo: &Repository, count: usize) -> Result<Vec<String>> {
    let mut messages = Vec::new();

    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;

    for oid in revwalk.take(count) {
        let oid = oid?;
        let commit = repo.find_commit(oid)?;
        if let Some(msg) = commit.message() {
            messages.push(msg.to_string());
        }
    }

    Ok(messages)
}
