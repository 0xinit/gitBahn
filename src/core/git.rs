//! Git operations using libgit2.

use std::path::Path;

use anyhow::{Context, Result};
use git2::{DiffOptions, Repository, StatusOptions};

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
