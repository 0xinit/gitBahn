//! Lock file management to prevent concurrent bahn instances.

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process;

use anyhow::{Context, Result};

const LOCK_FILE: &str = ".bahn.lock";

/// A guard that removes the lock file when dropped
pub struct LockGuard {
    path: PathBuf,
}

impl LockGuard {
    /// Acquire a lock for the given repository path
    pub fn acquire(repo_path: &std::path::Path) -> Result<Self> {
        let lock_path = repo_path.join(LOCK_FILE);

        // Check if lock file exists
        if lock_path.exists() {
            let content = fs::read_to_string(&lock_path)
                .unwrap_or_default();

            // Try to parse PID
            if let Some(pid_str) = content.lines().next() {
                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                    // Check if process is still running
                    if is_process_running(pid) {
                        anyhow::bail!(
                            "Another bahn instance is already running (PID: {}). \
                            If this is incorrect, remove {}",
                            pid,
                            lock_path.display()
                        );
                    }
                }
            }

            // Stale lock file, remove it
            let _ = fs::remove_file(&lock_path);
        }

        // Create lock file with our PID
        let mut file = File::create(&lock_path)
            .with_context(|| format!("Failed to create lock file: {}", lock_path.display()))?;

        writeln!(file, "{}", process::id())?;

        Ok(Self { path: lock_path })
    }

    /// Get the lock file path
    #[allow(dead_code)]
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        // Remove lock file on drop
        let _ = fs::remove_file(&self.path);
    }
}

/// Check if a process with the given PID is running
#[cfg(unix)]
fn is_process_running(pid: u32) -> bool {
    use std::process::Command;

    // Use kill -0 to check if process exists
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn is_process_running(pid: u32) -> bool {
    use std::process::Command;

    // Use tasklist to check if process exists
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {}", pid)])
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .contains(&pid.to_string())
        })
        .unwrap_or(false)
}
