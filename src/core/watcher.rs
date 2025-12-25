//! File system watcher for auto-commit mode.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};

/// Events emitted by the file watcher
#[derive(Debug, Clone)]
pub enum WatchEvent {
    /// Files were changed (debounced batch)
    FilesChanged(Vec<PathBuf>),
    /// Watcher error occurred
    Error(String),
}

/// File system watcher with debouncing
pub struct FileWatcher {
    /// Debounce duration for batching events
    debounce_duration: Duration,
    /// Patterns to ignore (e.g., .git, node_modules)
    ignore_patterns: Vec<String>,
}

impl FileWatcher {
    /// Create a new file watcher
    pub fn new(debounce_ms: u64) -> Self {
        Self {
            debounce_duration: Duration::from_millis(debounce_ms),
            ignore_patterns: vec![
                ".git".to_string(),
                "node_modules".to_string(),
                "target".to_string(),
                ".bahn.lock".to_string(),
                ".bahn.toml".to_string(),
            ],
        }
    }

    /// Add patterns to ignore
    #[allow(dead_code)]
    pub fn with_ignore_patterns(mut self, patterns: Vec<String>) -> Self {
        self.ignore_patterns.extend(patterns);
        self
    }

    /// Watch a directory and return a receiver for events
    pub fn watch(&self, path: PathBuf) -> Result<mpsc::Receiver<WatchEvent>> {
        let (tx, rx) = mpsc::channel();
        let ignore_patterns = self.ignore_patterns.clone();

        let (debounce_tx, debounce_rx) = mpsc::channel();

        // Create debounced watcher
        let mut debouncer = new_debouncer(
            self.debounce_duration,
            move |res: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
                match res {
                    Ok(events) => {
                        let _ = debounce_tx.send(Ok(events));
                    }
                    Err(e) => {
                        let _ = debounce_tx.send(Err(e));
                    }
                }
            },
        ).context("Failed to create file watcher")?;

        // Start watching
        debouncer.watcher().watch(&path, RecursiveMode::Recursive)
            .context("Failed to watch directory")?;

        // Spawn thread to process debounced events
        let tx_clone = tx.clone();
        std::thread::spawn(move || {
            // Keep debouncer alive
            let _debouncer = debouncer;

            loop {
                match debounce_rx.recv() {
                    Ok(Ok(events)) => {
                        let paths: Vec<PathBuf> = events
                            .into_iter()
                            .filter(|e| e.kind == DebouncedEventKind::Any)
                            .map(|e| e.path)
                            .filter(|p| {
                                let path_str = p.to_string_lossy();
                                !ignore_patterns.iter().any(|pattern| path_str.contains(pattern))
                            })
                            .collect();

                        if !paths.is_empty() {
                            let _ = tx_clone.send(WatchEvent::FilesChanged(paths));
                        }
                    }
                    Ok(Err(e)) => {
                        let _ = tx_clone.send(WatchEvent::Error(e.to_string()));
                    }
                    Err(_) => {
                        // Channel closed, exit
                        break;
                    }
                }
            }
        });

        Ok(rx)
    }
}

/// Simple watcher that uses notify directly (alternative to debounced)
#[allow(dead_code)]
pub struct SimpleWatcher {
    ignore_patterns: Vec<String>,
}

#[allow(dead_code)]
impl SimpleWatcher {
    /// Create a new simple watcher
    pub fn new() -> Self {
        Self {
            ignore_patterns: vec![
                ".git".to_string(),
                "node_modules".to_string(),
                "target".to_string(),
                ".bahn.lock".to_string(),
            ],
        }
    }

    /// Watch and return receiver
    pub fn watch(&self, path: PathBuf) -> Result<(mpsc::Receiver<Event>, RecommendedWatcher)> {
        let (tx, rx) = mpsc::channel();
        let ignore_patterns = self.ignore_patterns.clone();

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                // Filter out ignored paths
                let dominated_by_ignored = event.paths.iter().all(|p| {
                    let path_str = p.to_string_lossy();
                    ignore_patterns.iter().any(|pattern| path_str.contains(pattern))
                });

                if !dominated_by_ignored {
                    let _ = tx.send(event);
                }
            }
        }).context("Failed to create watcher")?;

        watcher.watch(&path, RecursiveMode::Recursive)
            .context("Failed to watch directory")?;

        Ok((rx, watcher))
    }
}

impl Default for SimpleWatcher {
    fn default() -> Self {
        Self::new()
    }
}
