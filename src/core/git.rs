//! Git operations using libgit2.

use std::path::Path;
use std::process::{Command, Stdio};
use std::io::Write;

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use git2::{DiffOptions, IndexAddOption, Repository, Signature, StatusOptions, Time};

/// A single hunk (chunk) of changes within a file
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DiffHunk {
    /// Unique identifier for this hunk
    pub id: usize,
    /// File path this hunk belongs to
    pub file_path: String,
    /// Whether this is a new file
    pub is_new_file: bool,
    /// Whether this is a deleted file
    pub is_deleted: bool,
    /// The hunk header (e.g., "@@ -10,6 +10,10 @@ fn main()")
    pub header: String,
    /// The actual diff content for this hunk
    pub content: String,
    /// Number of lines added in this hunk
    pub additions: usize,
    /// Number of lines deleted in this hunk
    pub deletions: usize,
    /// Context/description of what this hunk does (for AI)
    pub context: String,
}

impl DiffHunk {
    /// Get a summary for display
    #[allow(dead_code)]
    pub fn summary(&self) -> String {
        format!("{}:{} (+{}, -{})",
            self.file_path,
            self.header.split("@@").nth(1).unwrap_or("").trim(),
            self.additions,
            self.deletions
        )
    }
}

/// Parse staged changes into individual hunks
pub fn parse_diff_into_hunks(diff: &str) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();
    let mut current_file = String::new();
    let mut is_new_file = false;
    let mut is_deleted = false;
    let mut current_hunk_header = String::new();
    let mut current_hunk_content = String::new();
    let mut hunk_id = 0;
    let mut in_hunk = false;

    for line in diff.lines() {
        // New file header
        if line.starts_with("diff --git") {
            // Save previous hunk if exists
            if in_hunk && !current_hunk_content.is_empty() {
                let (additions, deletions) = count_changes(&current_hunk_content);
                hunks.push(DiffHunk {
                    id: hunk_id,
                    file_path: current_file.clone(),
                    is_new_file,
                    is_deleted,
                    header: current_hunk_header.clone(),
                    content: current_hunk_content.clone(),
                    additions,
                    deletions,
                    context: extract_hunk_context(&current_hunk_header, &current_hunk_content),
                });
                hunk_id += 1;
            }

            // Extract file path from "diff --git a/path b/path"
            let parts: Vec<&str> = line.split(' ').collect();
            if parts.len() >= 4 {
                current_file = parts[3].trim_start_matches("b/").to_string();
            }
            is_new_file = false;
            is_deleted = false;
            in_hunk = false;
            current_hunk_content.clear();
            current_hunk_header.clear();
        } else if line.starts_with("new file mode") {
            is_new_file = true;
        } else if line.starts_with("deleted file mode") {
            is_deleted = true;
        } else if line.starts_with("@@") {
            // Save previous hunk if exists
            if in_hunk && !current_hunk_content.is_empty() {
                let (additions, deletions) = count_changes(&current_hunk_content);
                hunks.push(DiffHunk {
                    id: hunk_id,
                    file_path: current_file.clone(),
                    is_new_file,
                    is_deleted,
                    header: current_hunk_header.clone(),
                    content: current_hunk_content.clone(),
                    additions,
                    deletions,
                    context: extract_hunk_context(&current_hunk_header, &current_hunk_content),
                });
                hunk_id += 1;
            }

            // Start new hunk
            current_hunk_header = line.to_string();
            current_hunk_content = format!("{}\n", line);
            in_hunk = true;
        } else if in_hunk {
            // Add line to current hunk
            current_hunk_content.push_str(line);
            current_hunk_content.push('\n');
        }
    }

    // Don't forget the last hunk
    if in_hunk && !current_hunk_content.is_empty() {
        let (additions, deletions) = count_changes(&current_hunk_content);
        let context = extract_hunk_context(&current_hunk_header, &current_hunk_content);
        hunks.push(DiffHunk {
            id: hunk_id,
            file_path: current_file,
            is_new_file,
            is_deleted,
            header: current_hunk_header,
            content: current_hunk_content,
            additions,
            deletions,
            context,
        });
    }

    hunks
}

/// Count additions and deletions in a hunk
fn count_changes(content: &str) -> (usize, usize) {
    let mut additions = 0;
    let mut deletions = 0;
    for line in content.lines() {
        if line.starts_with('+') && !line.starts_with("+++") {
            additions += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            deletions += 1;
        }
    }
    (additions, deletions)
}

/// Extract context/description from hunk for AI understanding
fn extract_hunk_context(header: &str, content: &str) -> String {
    // Try to get function/class context from header (e.g., "@@ -10,6 +10,10 @@ fn main()")
    let func_context = header.split("@@").nth(2).unwrap_or("").trim();

    // Get first few meaningful lines of added content
    let added_lines: Vec<&str> = content.lines()
        .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
        .take(3)
        .map(|l| l.trim_start_matches('+').trim())
        .filter(|l| !l.is_empty())
        .collect();

    if !func_context.is_empty() {
        format!("{}: {}", func_context, added_lines.join("; "))
    } else {
        added_lines.join("; ")
    }
}

/// Build a patch for specific hunks and apply it to the index
pub fn stage_hunks(repo_path: &Path, hunks: &[&DiffHunk]) -> Result<()> {
    if hunks.is_empty() {
        return Ok(());
    }

    // Group hunks by file
    let mut files_hunks: std::collections::HashMap<&str, Vec<&DiffHunk>> = std::collections::HashMap::new();
    for hunk in hunks {
        files_hunks.entry(&hunk.file_path).or_default().push(hunk);
    }

    for (file_path, file_hunks) in files_hunks {
        // Check if this is a new file (all hunks are from new file)
        let is_new_file = file_hunks.iter().all(|h| h.is_new_file);

        if is_new_file {
            // For new files, just stage the whole file
            Command::new("git")
                .args(["add", file_path])
                .current_dir(repo_path)
                .output()
                .context("Failed to stage new file")?;
        } else {
            // Build a patch for this file's hunks
            let patch = build_patch_for_hunks(file_path, &file_hunks);

            // Apply patch to index using git apply --cached
            let mut child = Command::new("git")
                .args(["apply", "--cached", "--unidiff-zero", "-"])
                .current_dir(repo_path)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .context("Failed to spawn git apply")?;

            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(patch.as_bytes())
                    .context("Failed to write patch to git apply")?;
            }

            let output = child.wait_with_output()
                .context("Failed to wait for git apply")?;

            if !output.status.success() {
                // If patch apply fails, fall back to staging the whole file
                // This can happen with complex changes
                Command::new("git")
                    .args(["add", file_path])
                    .current_dir(repo_path)
                    .output()
                    .context("Failed to stage file")?;
            }
        }
    }

    Ok(())
}

/// Build a git patch for specific hunks of a file
fn build_patch_for_hunks(file_path: &str, hunks: &[&DiffHunk]) -> String {
    let mut patch = String::new();

    // Patch header
    patch.push_str(&format!("diff --git a/{} b/{}\n", file_path, file_path));
    patch.push_str(&format!("--- a/{}\n", file_path));
    patch.push_str(&format!("+++ b/{}\n", file_path));

    // Add each hunk
    for hunk in hunks {
        patch.push_str(&hunk.content);
    }

    patch
}

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
    create_commit_at(repo, message, sign, None)
}

/// Create a commit with a specific timestamp
pub fn create_commit_at(
    repo: &Repository,
    message: &str,
    sign: bool,
    timestamp: Option<DateTime<Local>>,
) -> Result<git2::Oid> {
    let config = repo.config()?;
    let name = config.get_string("user.name")
        .unwrap_or_else(|_| "Unknown".to_string());
    let email = config.get_string("user.email")
        .unwrap_or_else(|_| "unknown@example.com".to_string());

    let signature = if let Some(ts) = timestamp {
        // Create signature with custom timestamp
        let time = Time::new(ts.timestamp(), ts.offset().local_minus_utc() / 60);
        Signature::new(&name, &email, &time)?
    } else {
        repo.signature()?
    };

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
    // Handle unborn branch (no commits yet)
    match repo.head() {
        Ok(head) => {
            if let Ok(commit) = head.peel_to_commit() {
                repo.reset(commit.as_object(), git2::ResetType::Mixed, None)?;
            }
        }
        Err(_) => {
            // Unborn branch - just clear the index
            let mut index = repo.index()?;
            index.clear()?;
            index.write()?;
        }
    }
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

    // Handle unborn branch (no commits yet)
    let head = match repo.head() {
        Ok(h) => h,
        Err(_) => return Ok(messages), // Return empty for new repos
    };

    if head.target().is_none() {
        return Ok(messages);
    }

    let mut revwalk = repo.revwalk()?;
    if revwalk.push_head().is_err() {
        return Ok(messages);
    }

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
    // Handle unborn branch (no commits yet)
    let head = match repo.head() {
        Ok(h) => h,
        Err(_) => {
            // Try to get branch name from HEAD reference
            if let Ok(head_ref) = repo.find_reference("HEAD") {
                if let Some(target) = head_ref.symbolic_target() {
                    // Extract branch name from refs/heads/master -> master
                    if let Some(branch) = target.strip_prefix("refs/heads/") {
                        return Ok(branch.to_string());
                    }
                }
            }
            return Ok("master".to_string()); // Default fallback
        }
    };

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
    // Handle unborn branch (no commits yet)
    let head = match repo.head() {
        Ok(h) => h,
        Err(_) => return Ok(0),
    };

    if head.target().is_none() {
        return Ok(0);
    }

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
    if revwalk.push_head().is_err() {
        return Ok(0);
    }
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

    // Handle unborn branch
    if repo.head().is_err() {
        return Ok(messages);
    }

    let mut revwalk = repo.revwalk()?;
    if revwalk.push_head().is_err() {
        return Ok(messages);
    }

    for oid in revwalk.take(count) {
        let oid = oid?;
        let commit = repo.find_commit(oid)?;
        if let Some(msg) = commit.message() {
            messages.push(msg.to_string());
        }
    }

    Ok(messages)
}
