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

// ============================================================================
// Realistic Mode: File Chunking for Progressive Commits
// ============================================================================

/// Type of logical chunk within a file
#[derive(Debug, Clone, PartialEq)]
pub enum ChunkType {
    /// Import statements
    Imports,
    /// Constants, configs, module-level variables
    Constants,
    /// Class/struct definition (just the signature and __init__/new)
    ClassDefinition,
    /// Individual method or function
    Function,
    /// Full file (for small files)
    FullFile,
    /// Misc code block
    Other,
}

impl std::fmt::Display for ChunkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChunkType::Imports => write!(f, "imports"),
            ChunkType::Constants => write!(f, "constants"),
            ChunkType::ClassDefinition => write!(f, "class"),
            ChunkType::Function => write!(f, "function"),
            ChunkType::FullFile => write!(f, "full"),
            ChunkType::Other => write!(f, "other"),
        }
    }
}

/// A logical chunk of a file (for realistic progressive commits)
#[derive(Debug, Clone)]
pub struct FileChunk {
    /// Unique ID for this chunk
    pub id: usize,
    /// File path
    pub file_path: String,
    /// Start line (1-indexed)
    pub start_line: usize,
    /// End line (1-indexed, inclusive)
    pub end_line: usize,
    /// The actual content of this chunk
    pub content: String,
    /// Type of chunk
    pub chunk_type: ChunkType,
    /// Human-readable description
    pub description: String,
    /// Number of lines
    pub line_count: usize,
    /// Dependencies (other files this chunk imports/uses)
    pub dependencies: Vec<String>,
}

/// Result of parsing all staged files into chunks
#[derive(Debug)]
pub struct ChunkedFiles {
    pub chunks: Vec<FileChunk>,
    pub file_order: Vec<String>,  // Suggested order based on dependencies
}

/// Parse all staged new files into logical chunks for realistic commits
pub fn parse_files_into_chunks(repo: &Repository) -> Result<ChunkedFiles> {
    let changes = get_staged_changes(repo)?;
    let workdir = repo.workdir().context("No working directory")?;

    let mut all_chunks = Vec::new();
    let mut chunk_id = 0;

    // Process added files (new files that can be chunked)
    for file_path in &changes.added {
        let full_path = workdir.join(file_path);
        let content = std::fs::read_to_string(&full_path)
            .unwrap_or_default();

        if content.is_empty() {
            continue;
        }

        let file_chunks = parse_single_file_into_chunks(
            file_path,
            &content,
            &mut chunk_id,
        );
        all_chunks.extend(file_chunks);
    }

    // Process modified files using their diff hunks
    for file_path in &changes.modified {
        let full_path = workdir.join(file_path);
        let content = std::fs::read_to_string(&full_path)
            .unwrap_or_default();

        if content.is_empty() {
            continue;
        }

        // For modified files, treat as single chunk for now
        // (could be enhanced to split by hunks)
        all_chunks.push(FileChunk {
            id: chunk_id,
            file_path: file_path.clone(),
            start_line: 1,
            end_line: content.lines().count(),
            content: content.clone(),
            chunk_type: ChunkType::FullFile,
            description: format!("Modified: {}", file_path),
            line_count: content.lines().count(),
            dependencies: extract_dependencies(&content, file_path),
        });
        chunk_id += 1;
    }

    // Determine file order based on dependencies
    let file_order = determine_file_order(&all_chunks);

    Ok(ChunkedFiles {
        chunks: all_chunks,
        file_order,
    })
}

/// Parse a single file into logical chunks based on its structure
fn parse_single_file_into_chunks(
    file_path: &str,
    content: &str,
    chunk_id: &mut usize,
) -> Vec<FileChunk> {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    // For small files (< 50 lines), keep as single chunk
    if total_lines < 50 {
        let id = *chunk_id;
        *chunk_id += 1;
        return vec![FileChunk {
            id,
            file_path: file_path.to_string(),
            start_line: 1,
            end_line: total_lines,
            content: content.to_string(),
            chunk_type: ChunkType::FullFile,
            description: format!("Add {}", file_path.split('/').last().unwrap_or(file_path)),
            line_count: total_lines,
            dependencies: extract_dependencies(content, file_path),
        }];
    }

    // Detect language and parse accordingly
    let ext = file_path.split('.').last().unwrap_or("");

    match ext {
        "py" => parse_python_file(file_path, &lines, content, chunk_id),
        "rs" => parse_rust_file(file_path, &lines, content, chunk_id),
        "js" | "ts" | "jsx" | "tsx" => parse_js_file(file_path, &lines, content, chunk_id),
        "go" => parse_go_file(file_path, &lines, content, chunk_id),
        _ => {
            // Generic: split into ~50 line chunks
            parse_generic_file(file_path, &lines, content, chunk_id)
        }
    }
}

/// Parse Python file into logical chunks
fn parse_python_file(
    file_path: &str,
    lines: &[&str],
    full_content: &str,
    chunk_id: &mut usize,
) -> Vec<FileChunk> {
    let mut chunks = Vec::new();
    let mut current_section_start = 0;
    let mut current_section_type = ChunkType::Imports;
    let mut in_class = false;
    let mut class_indent = 0;
    let mut current_class_name = String::new();

    let file_name = file_path.split('/').last().unwrap_or(file_path);

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let indent = line.len() - line.trim_start().len();

        // Detect section boundaries
        let is_import = trimmed.starts_with("import ") || trimmed.starts_with("from ");
        let is_class = trimmed.starts_with("class ");
        let is_function = trimmed.starts_with("def ") || trimmed.starts_with("async def ");
        let is_constant = !trimmed.is_empty()
            && !trimmed.starts_with('#')
            && !is_import
            && !is_class
            && !is_function
            && indent == 0
            && (trimmed.contains('=') || trimmed.chars().next().map(|c| c.is_uppercase()).unwrap_or(false));

        // Handle class definitions
        if is_class {
            // Save previous section
            if i > current_section_start {
                chunks.push(create_chunk(
                    file_path,
                    lines,
                    current_section_start,
                    i - 1,
                    current_section_type.clone(),
                    chunk_id,
                    &current_class_name,
                    file_name,
                ));
            }

            in_class = true;
            class_indent = indent;
            current_class_name = trimmed
                .trim_start_matches("class ")
                .split(['(', ':'])
                .next()
                .unwrap_or("Class")
                .to_string();
            current_section_start = i;
            current_section_type = ChunkType::ClassDefinition;
            continue;
        }

        // Handle top-level functions
        if is_function && indent == 0 {
            if i > current_section_start {
                chunks.push(create_chunk(
                    file_path,
                    lines,
                    current_section_start,
                    i - 1,
                    current_section_type.clone(),
                    chunk_id,
                    &current_class_name,
                    file_name,
                ));
            }
            in_class = false;
            current_class_name.clear();
            current_section_start = i;
            current_section_type = ChunkType::Function;
            continue;
        }

        // Handle methods inside class
        if in_class && is_function && indent > class_indent {
            // Only split if we have substantial content
            if i > current_section_start + 5 {
                chunks.push(create_chunk(
                    file_path,
                    lines,
                    current_section_start,
                    i - 1,
                    current_section_type.clone(),
                    chunk_id,
                    &current_class_name,
                    file_name,
                ));
                current_section_start = i;
                current_section_type = ChunkType::Function;
            }
            continue;
        }

        // Detect transition from imports to constants
        if current_section_type == ChunkType::Imports && !is_import && !trimmed.is_empty() && !trimmed.starts_with('#') {
            if i > current_section_start {
                chunks.push(create_chunk(
                    file_path,
                    lines,
                    current_section_start,
                    i - 1,
                    ChunkType::Imports,
                    chunk_id,
                    "",
                    file_name,
                ));
            }
            current_section_start = i;
            current_section_type = if is_constant { ChunkType::Constants } else { ChunkType::Other };
        }
    }

    // Don't forget the last section
    if current_section_start < lines.len() {
        chunks.push(create_chunk(
            file_path,
            lines,
            current_section_start,
            lines.len() - 1,
            current_section_type,
            chunk_id,
            &current_class_name,
            file_name,
        ));
    }

    // If we only got 1 chunk, it's essentially the same as full file
    if chunks.len() == 1 {
        chunks[0].chunk_type = ChunkType::FullFile;
    }

    // Add dependencies to first chunk
    if !chunks.is_empty() {
        chunks[0].dependencies = extract_dependencies(full_content, file_path);
    }

    chunks
}

/// Parse Rust file into logical chunks
fn parse_rust_file(
    file_path: &str,
    lines: &[&str],
    full_content: &str,
    chunk_id: &mut usize,
) -> Vec<FileChunk> {
    let mut chunks = Vec::new();
    let mut current_section_start = 0;
    let mut current_section_type = ChunkType::Imports;
    let mut brace_depth = 0;

    let file_name = file_path.split('/').last().unwrap_or(file_path);

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Count braces for tracking blocks
        brace_depth += trimmed.matches('{').count() as i32;
        brace_depth -= trimmed.matches('}').count() as i32;

        let is_use = trimmed.starts_with("use ");
        let is_struct = trimmed.starts_with("pub struct ") || trimmed.starts_with("struct ");
        let is_impl = trimmed.starts_with("impl ");
        let is_fn = trimmed.starts_with("pub fn ") || trimmed.starts_with("fn ")
            || trimmed.starts_with("pub async fn ") || trimmed.starts_with("async fn ");
        let is_const = trimmed.starts_with("const ") || trimmed.starts_with("pub const ")
            || trimmed.starts_with("static ") || trimmed.starts_with("pub static ");

        // Detect major section boundaries (only at top level)
        if brace_depth == 0 || (brace_depth == 1 && trimmed.contains('{')) {
            if is_struct || is_impl || is_fn {
                if i > current_section_start + 2 {
                    chunks.push(create_chunk(
                        file_path,
                        lines,
                        current_section_start,
                        i - 1,
                        current_section_type.clone(),
                        chunk_id,
                        "",
                        file_name,
                    ));
                    current_section_start = i;
                }

                current_section_type = if is_struct {
                    ChunkType::ClassDefinition
                } else if is_fn {
                    ChunkType::Function
                } else {
                    ChunkType::Other
                };
            }
        }

        // Transition from use statements
        if current_section_type == ChunkType::Imports && !is_use && !trimmed.is_empty()
            && !trimmed.starts_with("//") && !trimmed.starts_with("#[") {
            if i > current_section_start {
                chunks.push(create_chunk(
                    file_path,
                    lines,
                    current_section_start,
                    i - 1,
                    ChunkType::Imports,
                    chunk_id,
                    "",
                    file_name,
                ));
            }
            current_section_start = i;
            current_section_type = if is_const { ChunkType::Constants } else { ChunkType::Other };
        }
    }

    // Last section
    if current_section_start < lines.len() {
        chunks.push(create_chunk(
            file_path,
            lines,
            current_section_start,
            lines.len() - 1,
            current_section_type,
            chunk_id,
            "",
            file_name,
        ));
    }

    if chunks.len() == 1 {
        chunks[0].chunk_type = ChunkType::FullFile;
    }

    if !chunks.is_empty() {
        chunks[0].dependencies = extract_dependencies(full_content, file_path);
    }

    chunks
}

/// Parse JavaScript/TypeScript file into logical chunks
fn parse_js_file(
    file_path: &str,
    lines: &[&str],
    full_content: &str,
    chunk_id: &mut usize,
) -> Vec<FileChunk> {
    let mut chunks = Vec::new();
    let mut current_section_start = 0;
    let mut current_section_type = ChunkType::Imports;
    let mut brace_depth = 0;

    let file_name = file_path.split('/').last().unwrap_or(file_path);

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        brace_depth += trimmed.matches('{').count() as i32;
        brace_depth -= trimmed.matches('}').count() as i32;

        let is_import = trimmed.starts_with("import ");
        let is_class = trimmed.starts_with("class ") || trimmed.starts_with("export class ");
        let is_function = trimmed.starts_with("function ") || trimmed.starts_with("export function ")
            || trimmed.starts_with("const ") && trimmed.contains("=>")
            || trimmed.starts_with("export const ") && trimmed.contains("=>");
        let is_export = trimmed.starts_with("export ");

        if brace_depth == 0 && (is_class || is_function) {
            if i > current_section_start + 2 {
                chunks.push(create_chunk(
                    file_path,
                    lines,
                    current_section_start,
                    i - 1,
                    current_section_type.clone(),
                    chunk_id,
                    "",
                    file_name,
                ));
                current_section_start = i;
            }
            current_section_type = if is_class { ChunkType::ClassDefinition } else { ChunkType::Function };
        }

        if current_section_type == ChunkType::Imports && !is_import && !trimmed.is_empty()
            && !trimmed.starts_with("//") && !trimmed.starts_with("/*") {
            if i > current_section_start {
                chunks.push(create_chunk(
                    file_path,
                    lines,
                    current_section_start,
                    i - 1,
                    ChunkType::Imports,
                    chunk_id,
                    "",
                    file_name,
                ));
            }
            current_section_start = i;
            current_section_type = ChunkType::Other;
        }
    }

    if current_section_start < lines.len() {
        chunks.push(create_chunk(
            file_path,
            lines,
            current_section_start,
            lines.len() - 1,
            current_section_type,
            chunk_id,
            "",
            file_name,
        ));
    }

    if chunks.len() == 1 {
        chunks[0].chunk_type = ChunkType::FullFile;
    }

    if !chunks.is_empty() {
        chunks[0].dependencies = extract_dependencies(full_content, file_path);
    }

    chunks
}

/// Parse Go file into logical chunks
fn parse_go_file(
    file_path: &str,
    lines: &[&str],
    full_content: &str,
    chunk_id: &mut usize,
) -> Vec<FileChunk> {
    let mut chunks = Vec::new();
    let mut current_section_start = 0;
    let mut current_section_type = ChunkType::Imports;
    let mut brace_depth = 0;

    let file_name = file_path.split('/').last().unwrap_or(file_path);

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        brace_depth += trimmed.matches('{').count() as i32;
        brace_depth -= trimmed.matches('}').count() as i32;

        let is_import = trimmed.starts_with("import ");
        let is_package = trimmed.starts_with("package ");
        let is_type = trimmed.starts_with("type ");
        let is_func = trimmed.starts_with("func ");
        let is_const = trimmed.starts_with("const ") || trimmed.starts_with("var ");

        if brace_depth == 0 && (is_type || is_func) {
            if i > current_section_start + 2 {
                chunks.push(create_chunk(
                    file_path,
                    lines,
                    current_section_start,
                    i - 1,
                    current_section_type.clone(),
                    chunk_id,
                    "",
                    file_name,
                ));
                current_section_start = i;
            }
            current_section_type = if is_type { ChunkType::ClassDefinition } else { ChunkType::Function };
        }

        if current_section_type == ChunkType::Imports && !is_import && !is_package && !trimmed.is_empty()
            && !trimmed.starts_with("//") && trimmed != ")" {
            if i > current_section_start {
                chunks.push(create_chunk(
                    file_path,
                    lines,
                    current_section_start,
                    i - 1,
                    ChunkType::Imports,
                    chunk_id,
                    "",
                    file_name,
                ));
            }
            current_section_start = i;
            current_section_type = if is_const { ChunkType::Constants } else { ChunkType::Other };
        }
    }

    if current_section_start < lines.len() {
        chunks.push(create_chunk(
            file_path,
            lines,
            current_section_start,
            lines.len() - 1,
            current_section_type,
            chunk_id,
            "",
            file_name,
        ));
    }

    if chunks.len() == 1 {
        chunks[0].chunk_type = ChunkType::FullFile;
    }

    if !chunks.is_empty() {
        chunks[0].dependencies = extract_dependencies(full_content, file_path);
    }

    chunks
}

/// Parse generic file by splitting into ~50 line chunks
fn parse_generic_file(
    file_path: &str,
    lines: &[&str],
    _full_content: &str,
    chunk_id: &mut usize,
) -> Vec<FileChunk> {
    let mut chunks = Vec::new();
    let chunk_size = 50;
    let file_name = file_path.split('/').last().unwrap_or(file_path);

    let mut start = 0;
    while start < lines.len() {
        let end = (start + chunk_size).min(lines.len()) - 1;
        let id = *chunk_id;
        *chunk_id += 1;

        let content: String = lines[start..=end].join("\n");

        chunks.push(FileChunk {
            id,
            file_path: file_path.to_string(),
            start_line: start + 1,
            end_line: end + 1,
            content,
            chunk_type: if start == 0 { ChunkType::FullFile } else { ChunkType::Other },
            description: format!(
                "{} lines {}-{}",
                file_name,
                start + 1,
                end + 1
            ),
            line_count: end - start + 1,
            dependencies: vec![],
        });

        start = end + 1;
    }

    chunks
}

/// Create a chunk from line range
fn create_chunk(
    file_path: &str,
    lines: &[&str],
    start: usize,
    end: usize,
    chunk_type: ChunkType,
    chunk_id: &mut usize,
    class_name: &str,
    file_name: &str,
) -> FileChunk {
    let id = *chunk_id;
    *chunk_id += 1;

    let content: String = lines[start..=end.min(lines.len() - 1)].join("\n");
    let line_count = end - start + 1;

    // Generate description based on chunk type
    let description = match chunk_type {
        ChunkType::Imports => format!("Add imports for {}", file_name),
        ChunkType::Constants => format!("Add constants for {}", file_name),
        ChunkType::ClassDefinition => {
            if class_name.is_empty() {
                format!("Add class definition in {}", file_name)
            } else {
                format!("Add {} class structure", class_name)
            }
        }
        ChunkType::Function => {
            // Try to extract function name
            let func_line = lines.get(start).unwrap_or(&"");
            let func_name = extract_function_name(func_line);
            if func_name.is_empty() {
                format!("Add function in {}", file_name)
            } else if !class_name.is_empty() {
                format!("Add {}.{} method", class_name, func_name)
            } else {
                format!("Add {} function", func_name)
            }
        }
        ChunkType::FullFile => format!("Add {}", file_name),
        ChunkType::Other => format!("Add code in {}", file_name),
    };

    FileChunk {
        id,
        file_path: file_path.to_string(),
        start_line: start + 1,
        end_line: end + 1,
        content,
        chunk_type,
        description,
        line_count,
        dependencies: vec![],
    }
}

/// Extract function name from a function definition line
fn extract_function_name(line: &str) -> String {
    let trimmed = line.trim();

    // Python: def func_name( or async def func_name(
    if trimmed.starts_with("def ") || trimmed.starts_with("async def ") {
        let start = if trimmed.starts_with("async") { "async def ".len() } else { "def ".len() };
        return trimmed[start..]
            .split('(')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
    }

    // Rust: fn func_name( or pub fn func_name(
    if trimmed.contains("fn ") {
        return trimmed
            .split("fn ")
            .nth(1)
            .unwrap_or("")
            .split(['(', '<'])
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
    }

    // JS: function func_name( or const func_name =
    if trimmed.starts_with("function ") {
        return trimmed["function ".len()..]
            .split('(')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
    }

    // Go: func funcName( or func (r *Receiver) funcName(
    if trimmed.starts_with("func ") {
        let after_func = &trimmed["func ".len()..];
        if after_func.starts_with('(') {
            // Method with receiver
            return after_func
                .split(')')
                .nth(1)
                .unwrap_or("")
                .trim()
                .split('(')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
        } else {
            return after_func
                .split('(')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
        }
    }

    String::new()
}

/// Extract dependencies (imports) from file content
fn extract_dependencies(content: &str, file_path: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let ext = file_path.split('.').last().unwrap_or("");

    for line in content.lines() {
        let trimmed = line.trim();

        match ext {
            "py" => {
                if trimmed.starts_with("from ") {
                    // from module import ...
                    if let Some(module) = trimmed.strip_prefix("from ") {
                        let module = module.split_whitespace().next().unwrap_or("");
                        if !module.is_empty() && !module.starts_with('.') {
                            deps.push(module.to_string());
                        }
                    }
                } else if trimmed.starts_with("import ") {
                    if let Some(module) = trimmed.strip_prefix("import ") {
                        let module = module.split([',', ' ']).next().unwrap_or("");
                        if !module.is_empty() {
                            deps.push(module.to_string());
                        }
                    }
                }
            }
            "rs" => {
                if trimmed.starts_with("use ") {
                    if let Some(path) = trimmed.strip_prefix("use ") {
                        let path = path.trim_end_matches(';').split("::").next().unwrap_or("");
                        if !path.is_empty() && path != "crate" && path != "self" && path != "super" {
                            deps.push(path.to_string());
                        }
                    }
                }
            }
            "js" | "ts" | "jsx" | "tsx" => {
                if trimmed.starts_with("import ") {
                    // import ... from "module"
                    if let Some(from_part) = trimmed.split(" from ").nth(1) {
                        let module = from_part.trim_matches(|c| c == '"' || c == '\'' || c == ';');
                        if !module.is_empty() {
                            deps.push(module.to_string());
                        }
                    }
                }
            }
            "go" => {
                if trimmed.starts_with("import ") || trimmed.starts_with('"') {
                    let module = trimmed.trim_matches(|c| c == '"' || c == ' ' || c == '\t');
                    if !module.is_empty() && module != "import" && module != "(" {
                        deps.push(module.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    deps
}

/// Determine optimal file order based on dependencies and file types
fn determine_file_order(chunks: &[FileChunk]) -> Vec<String> {
    let mut files: Vec<String> = chunks.iter()
        .map(|c| c.file_path.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    // Sort by priority:
    // 1. Config files (.gitignore, requirements.txt, package.json, Cargo.toml)
    // 2. Environment files (.env.example)
    // 3. Init files (__init__.py, mod.rs)
    // 4. Models/types (models.py, types.rs)
    // 5. Utils/helpers (utils.py, helpers.rs)
    // 6. Services/core logic
    // 7. Routes/handlers
    // 8. Main entry points
    // 9. Tests
    // 10. Docker/CI files
    // 11. Documentation (README)

    files.sort_by(|a, b| {
        let priority_a = file_priority(a);
        let priority_b = file_priority(b);
        priority_a.cmp(&priority_b)
    });

    files
}

/// Get priority for file ordering (lower = earlier)
fn file_priority(path: &str) -> u32 {
    let name = path.split('/').last().unwrap_or(path).to_lowercase();
    let dir = path.split('/').rev().nth(1).unwrap_or("").to_lowercase();

    // Config and setup files first
    if name == ".gitignore" || name == ".env.example" { return 1; }
    if name == "requirements.txt" || name == "package.json" || name == "cargo.toml" || name == "go.mod" { return 2; }
    if name == "pyproject.toml" || name == "setup.py" || name == "tsconfig.json" { return 3; }

    // Config modules
    if name.contains("config") { return 10; }
    if name.contains("settings") { return 11; }
    if name.contains("constants") { return 12; }

    // Init files
    if name == "__init__.py" || name == "mod.rs" || name == "index.ts" || name == "index.js" { return 15; }

    // Shared/common utilities
    if dir == "shared" || dir == "common" || dir == "utils" { return 20; }
    if name.contains("utils") || name.contains("helpers") { return 21; }

    // Models and types
    if name.contains("models") || name.contains("types") || name.contains("schemas") { return 30; }

    // Core services
    if dir == "services" || name.contains("service") { return 40; }
    if dir == "core" { return 41; }

    // Clients and integrations
    if name.contains("client") { return 50; }
    if dir == "indexers" || dir == "integrations" { return 51; }

    // Handlers and routes
    if dir == "routers" || dir == "routes" || dir == "handlers" { return 60; }
    if name.contains("router") || name.contains("handler") { return 61; }

    // Main entry points
    if name == "main.py" || name == "main.rs" || name == "main.go" || name == "app.py" { return 70; }
    if name == "index.ts" || name == "index.js" || name == "app.ts" { return 71; }

    // CLI
    if dir == "cli" || name.contains("cli") { return 75; }

    // Tests
    if name.starts_with("test_") || name.ends_with("_test.py") || name.ends_with("_test.go") { return 80; }
    if dir == "tests" || dir == "test" { return 81; }

    // Docker and deployment
    if name == "dockerfile" || name == "docker-compose.yml" || name == "docker-compose.yaml" { return 90; }
    if dir == ".github" || name.contains("ci") { return 91; }

    // Documentation last
    if name == "readme.md" || name == "readme.rst" { return 100; }
    if name.ends_with(".md") { return 101; }

    // Default
    50
}

/// Write partial file content for progressive commits
pub fn write_file_content(repo_path: &Path, file_path: &str, content: &str) -> Result<()> {
    let full_path = repo_path.join(file_path);

    // Ensure parent directory exists
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory for {}", file_path))?;
    }

    std::fs::write(&full_path, content)
        .with_context(|| format!("Failed to write {}", file_path))?;

    Ok(())
}

/// Stage a specific file
pub fn stage_file(repo_path: &Path, file_path: &str) -> Result<()> {
    Command::new("git")
        .args(["add", file_path])
        .current_dir(repo_path)
        .output()
        .with_context(|| format!("Failed to stage {}", file_path))?;
    Ok(())
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
