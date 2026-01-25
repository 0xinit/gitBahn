//! gitBahn MCP Server
//!
//! Thin git operations layer for Claude Code with smart splitting suggestions.
//! No AI calls - Claude Code handles commit message generation directly.

use std::process::Command;
use rmcp::{
    ErrorData as McpError,
    ServerHandler,
    model::*,
    tool, tool_router, tool_handler,
    handler::server::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    transport::io::stdio,
    ServiceExt,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StageFilesRequest {
    #[schemars(description = "List of file paths to stage")]
    pub files: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateCommitRequest {
    #[schemars(description = "The commit message")]
    pub message: String,
    #[schemars(description = "Optional timestamp (e.g., '2025-01-03 11:17:32')")]
    pub timestamp: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetDiffRequest {
    #[schemars(description = "Get staged changes only (default: true)")]
    pub staged: Option<bool>,
    #[schemars(description = "Optional list of specific files")]
    pub files: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetLogRequest {
    #[schemars(description = "Number of commits to show (default: 10)")]
    pub count: Option<u32>,
    #[schemars(description = "Show full commit messages")]
    pub full: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PushRequest {
    #[schemars(description = "Remote name (default: origin)")]
    pub remote: Option<String>,
    #[schemars(description = "Branch name (default: current)")]
    pub branch: Option<String>,
    #[schemars(description = "Force push (use with caution)")]
    pub force: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UndoRequest {
    #[schemars(description = "Number of commits to undo (default: 1)")]
    pub count: Option<u32>,
    #[schemars(description = "Hard reset - discard changes (default: false)")]
    pub hard: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SplitRequest {
    #[schemars(description = "Target number of commits (optional, will suggest optimal)")]
    pub target_commits: Option<u32>,
}

// Split suggestion response types
#[derive(Debug, Serialize)]
pub struct SplitGroup {
    pub group_id: usize,
    pub files: Vec<String>,
    pub description: String,
    pub hint: String,
    pub line_count: usize,
}

#[derive(Debug, Serialize)]
pub struct SplitSuggestion {
    pub total_groups: usize,
    pub groups: Vec<SplitGroup>,
    pub suggested_order: Vec<usize>,
}

// ============================================================================
// Server Implementation
// ============================================================================

#[derive(Clone)]
pub struct GitBahnServer {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl GitBahnServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    // ========================================================================
    // Basic Git Operations
    // ========================================================================

    #[tool(description = "Get git status showing staged and unstaged changes")]
    async fn get_status(&self) -> Result<CallToolResult, McpError> {
        let result = run_git(&["status", "--short"]);
        let output = if result.is_empty() {
            "Working tree clean - no changes.".to_string()
        } else {
            format!("Status:\n{}\n\nLegend: M=modified, A=added, D=deleted, ??=untracked\nFirst column=staged, second=unstaged", result)
        };
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "Get diff of staged or unstaged changes")]
    async fn get_diff(&self, params: Parameters<GetDiffRequest>) -> Result<CallToolResult, McpError> {
        let req = params.0;
        let staged = req.staged.unwrap_or(true);
        let mut args = vec!["diff"];
        if staged { args.push("--cached"); }

        let files_str: Vec<&str>;
        if let Some(ref files) = req.files {
            args.push("--");
            files_str = files.iter().map(|s| s.as_str()).collect();
            args.extend(&files_str);
        }

        let result = run_git(&args);
        let output = if result.is_empty() {
            format!("No {} changes.", if staged { "staged" } else { "unstaged" })
        } else {
            result
        };
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "Stage all changes (git add -A)")]
    async fn stage_all(&self) -> Result<CallToolResult, McpError> {
        run_git(&["add", "-A"]);
        Ok(CallToolResult::success(vec![Content::text("All changes staged.".to_string())]))
    }

    #[tool(description = "Stage specific files")]
    async fn stage_files(&self, params: Parameters<StageFilesRequest>) -> Result<CallToolResult, McpError> {
        let req = params.0;
        if req.files.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("No files specified.".to_string())]));
        }
        let mut args = vec!["add", "--"];
        let files_ref: Vec<&str> = req.files.iter().map(|s| s.as_str()).collect();
        args.extend(files_ref);
        run_git(&args);
        Ok(CallToolResult::success(vec![Content::text(format!("Staged: {}", req.files.join(", ")))]))
    }

    #[tool(description = "Unstage all files (keep changes in working directory)")]
    async fn unstage_all(&self) -> Result<CallToolResult, McpError> {
        run_git(&["reset", "HEAD"]);
        Ok(CallToolResult::success(vec![Content::text("All files unstaged.".to_string())]))
    }

    #[tool(description = "Create a commit with the provided message. Optionally backdate.")]
    async fn create_commit(&self, params: Parameters<CreateCommitRequest>) -> Result<CallToolResult, McpError> {
        let req = params.0;
        let staged = run_git(&["diff", "--cached", "--stat"]);
        if staged.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("Nothing to commit - no staged changes.".to_string())]));
        }

        let result = if let Some(timestamp) = req.timestamp {
            let date_str = format!("{} +0000", timestamp);
            match Command::new("git")
                .args(["commit", "-m", &req.message])
                .env("GIT_AUTHOR_DATE", &date_str)
                .env("GIT_COMMITTER_DATE", &date_str)
                .output()
            {
                Ok(output) if output.status.success() => {
                    format!("Committed at {}:\n{}", timestamp, String::from_utf8_lossy(&output.stdout))
                }
                Ok(output) => format!("Failed: {}", String::from_utf8_lossy(&output.stderr)),
                Err(e) => format!("Error: {}", e),
            }
        } else {
            match Command::new("git").args(["commit", "-m", &req.message]).output() {
                Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout).to_string(),
                Ok(output) => format!("Failed: {}", String::from_utf8_lossy(&output.stderr)),
                Err(e) => format!("Error: {}", e),
            }
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Get recent commit history")]
    async fn get_log(&self, params: Parameters<GetLogRequest>) -> Result<CallToolResult, McpError> {
        let req = params.0;
        let count = req.count.unwrap_or(10).to_string();
        let format = if req.full.unwrap_or(false) { "%h %ci%n  %s%n  %b" } else { "%h %ci %s" };
        let result = run_git(&["log", &format!("-{}", count), &format!("--format={}", format)]);
        Ok(CallToolResult::success(vec![Content::text(if result.is_empty() { "No commits yet.".to_string() } else { result })]))
    }

    #[tool(description = "Get current branch name")]
    async fn get_branch(&self) -> Result<CallToolResult, McpError> {
        let result = run_git(&["branch", "--show-current"]);
        Ok(CallToolResult::success(vec![Content::text(format!("Branch: {}", result.trim()))]))
    }

    #[tool(description = "Push commits to remote")]
    async fn push(&self, params: Parameters<PushRequest>) -> Result<CallToolResult, McpError> {
        let req = params.0;
        let remote = req.remote.unwrap_or_else(|| "origin".to_string());
        let mut args = vec!["push".to_string()];
        if req.force.unwrap_or(false) { args.push("--force-with-lease".to_string()); }
        args.push(remote.clone());
        if let Some(branch) = req.branch { args.push(branch); }
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let result = run_git(&args_ref);
        Ok(CallToolResult::success(vec![Content::text(if result.is_empty() { format!("Pushed to {}", remote) } else { result })]))
    }

    #[tool(description = "Undo recent commits (soft reset keeps changes staged)")]
    async fn undo(&self, params: Parameters<UndoRequest>) -> Result<CallToolResult, McpError> {
        let req = params.0;
        let count = req.count.unwrap_or(1);
        let reset_type = if req.hard.unwrap_or(false) { "--hard" } else { "--soft" };
        run_git(&["reset", reset_type, &format!("HEAD~{}", count)]);
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Reset {} commit(s) ({})", count, if req.hard.unwrap_or(false) { "changes discarded" } else { "changes kept staged" }
        ))]))
    }

    #[tool(description = "List changed files grouped by status")]
    async fn list_changes(&self) -> Result<CallToolResult, McpError> {
        let status = run_git(&["status", "--porcelain"]);
        if status.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("No changes.".to_string())]));
        }

        let mut staged = Vec::new();
        let mut unstaged = Vec::new();
        let mut untracked = Vec::new();

        for line in status.lines() {
            if line.len() < 3 { continue; }
            let idx = line.chars().next().unwrap_or(' ');
            let wt = line.chars().nth(1).unwrap_or(' ');
            let file = &line[3..];

            if idx == '?' { untracked.push(file.to_string()); }
            else {
                if idx != ' ' { staged.push(format!("{} {}", idx, file)); }
                if wt != ' ' { unstaged.push(format!("{} {}", wt, file)); }
            }
        }

        let mut out = String::new();
        if !staged.is_empty() { out.push_str(&format!("Staged ({}):\n  {}\n", staged.len(), staged.join("\n  "))); }
        if !unstaged.is_empty() { out.push_str(&format!("\nUnstaged ({}):\n  {}\n", unstaged.len(), unstaged.join("\n  "))); }
        if !untracked.is_empty() { out.push_str(&format!("\nUntracked ({}):\n  {}\n", untracked.len(), untracked.join("\n  "))); }
        Ok(CallToolResult::success(vec![Content::text(out)]))
    }

    // ========================================================================
    // Smart Split Suggestions
    // ========================================================================

    #[tool(description = "Suggest realistic commit split: groups files by language constructs (imports, classes, functions) and orders by dependency. Best for new projects.")]
    async fn suggest_realistic_split(&self, params: Parameters<SplitRequest>) -> Result<CallToolResult, McpError> {
        let req = params.0;
        let files = get_staged_files();

        if files.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("No staged files to split.".to_string())]));
        }

        let mut groups: Vec<SplitGroup> = Vec::new();
        let mut group_id = 0;

        // Parse each file into chunks based on language
        for file in &files {
            let content = std::fs::read_to_string(file).unwrap_or_default();
            if content.is_empty() { continue; }

            let ext = file.split('.').last().unwrap_or("");
            let chunks = parse_file_chunks(file, &content, ext);

            for chunk in chunks {
                groups.push(SplitGroup {
                    group_id,
                    files: vec![file.clone()],
                    description: chunk.description,
                    hint: chunk.hint,
                    line_count: chunk.line_count,
                });
                group_id += 1;
            }
        }

        // Sort by dependency order: config -> utils -> core -> features -> tests -> docs
        groups.sort_by_key(|g| file_priority(&g.files[0]));

        // Optionally merge small groups if target_commits specified
        if let Some(target) = req.target_commits {
            groups = merge_groups_to_target(groups, target as usize);
        }

        // Update group IDs and create order
        let suggested_order: Vec<usize> = (0..groups.len()).collect();
        for (i, g) in groups.iter_mut().enumerate() {
            g.group_id = i;
        }

        let suggestion = SplitSuggestion {
            total_groups: groups.len(),
            groups,
            suggested_order,
        };

        Ok(CallToolResult::success(vec![Content::text(format_split_suggestion(&suggestion, "realistic"))]))
    }

    #[tool(description = "Suggest atomic commit split: each file becomes its own commit. Simple and quick.")]
    async fn suggest_atomic_split(&self, _params: Parameters<SplitRequest>) -> Result<CallToolResult, McpError> {
        let files = get_staged_files();

        if files.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("No staged files to split.".to_string())]));
        }

        let mut groups: Vec<SplitGroup> = Vec::new();

        for (i, file) in files.iter().enumerate() {
            let content = std::fs::read_to_string(file).unwrap_or_default();
            let line_count = content.lines().count();
            let ext = file.split('.').last().unwrap_or("");

            let (desc, hint) = get_file_description(file, &content, ext);

            groups.push(SplitGroup {
                group_id: i,
                files: vec![file.clone()],
                description: desc,
                hint,
                line_count,
            });
        }

        // Sort by dependency order
        groups.sort_by_key(|g| file_priority(&g.files[0]));
        for (i, g) in groups.iter_mut().enumerate() {
            g.group_id = i;
        }

        let suggested_order: Vec<usize> = (0..groups.len()).collect();
        let suggestion = SplitSuggestion {
            total_groups: groups.len(),
            groups,
            suggested_order,
        };

        Ok(CallToolResult::success(vec![Content::text(format_split_suggestion(&suggestion, "atomic"))]))
    }

    #[tool(description = "Suggest granular commit split: splits by diff hunks (changes within files). Allows splitting a single file across multiple commits. Best for modified files.")]
    async fn suggest_granular_split(&self, params: Parameters<SplitRequest>) -> Result<CallToolResult, McpError> {
        let req = params.0;

        // Get diff with hunks
        let diff = run_git(&["diff", "--cached", "-U3"]);
        if diff.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("No staged changes to split.".to_string())]));
        }

        let hunks = parse_diff_hunks(&diff);
        if hunks.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("No hunks found in diff.".to_string())]));
        }

        let mut groups: Vec<SplitGroup> = hunks.iter().enumerate().map(|(i, h)| {
            SplitGroup {
                group_id: i,
                files: vec![h.file.clone()],
                description: h.description.clone(),
                hint: format!("{}:{} (+{}/-{})", h.file, h.start_line, h.additions, h.deletions),
                line_count: h.additions + h.deletions,
            }
        }).collect();

        // Merge if target specified
        if let Some(target) = req.target_commits {
            groups = merge_groups_to_target(groups, target as usize);
        }

        for (i, g) in groups.iter_mut().enumerate() {
            g.group_id = i;
        }

        let suggested_order: Vec<usize> = (0..groups.len()).collect();
        let suggestion = SplitSuggestion {
            total_groups: groups.len(),
            groups,
            suggested_order,
        };

        Ok(CallToolResult::success(vec![Content::text(format_split_suggestion(&suggestion, "granular"))]))
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn run_git(args: &[&str]) -> String {
    match Command::new("git").args(args).output() {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout).to_string(),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.is_empty() { String::from_utf8_lossy(&output.stdout).to_string() }
            else { format!("Error: {}", stderr) }
        }
        Err(e) => format!("Failed: {}", e),
    }
}

fn get_staged_files() -> Vec<String> {
    let output = run_git(&["diff", "--cached", "--name-only"]);
    output.lines().map(|s| s.to_string()).filter(|s| !s.is_empty()).collect()
}

// File chunk for parsing
struct FileChunk {
    description: String,
    hint: String,
    line_count: usize,
}

// Parse file into logical chunks based on language
fn parse_file_chunks(file_path: &str, content: &str, ext: &str) -> Vec<FileChunk> {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    // Small files: single chunk
    if total_lines < 30 {
        return vec![FileChunk {
            description: format!("Add {}", file_path.split('/').last().unwrap_or(file_path)),
            hint: format!("{} ({} lines)", ext_to_type(ext), total_lines),
            line_count: total_lines,
        }];
    }

    match ext {
        "py" => parse_python_chunks(file_path, &lines),
        "rs" => parse_rust_chunks(file_path, &lines),
        "js" | "ts" | "jsx" | "tsx" => parse_js_chunks(file_path, &lines),
        "go" => parse_go_chunks(file_path, &lines),
        "rb" => parse_ruby_chunks(file_path, &lines),
        _ => vec![FileChunk {
            description: format!("Add {}", file_path.split('/').last().unwrap_or(file_path)),
            hint: format!("file ({} lines)", total_lines),
            line_count: total_lines,
        }],
    }
}

fn parse_python_chunks(file_path: &str, lines: &[&str]) -> Vec<FileChunk> {
    let mut chunks = Vec::new();
    let file_name = file_path.split('/').last().unwrap_or(file_path);

    let mut imports_end = 0;
    let mut has_classes = false;
    let mut has_functions = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
            imports_end = i + 1;
        }
        if trimmed.starts_with("class ") { has_classes = true; }
        if trimmed.starts_with("def ") || trimmed.starts_with("async def ") { has_functions = true; }
    }

    if imports_end > 0 {
        chunks.push(FileChunk {
            description: format!("Add imports for {}", file_name),
            hint: "imports".to_string(),
            line_count: imports_end,
        });
    }

    if has_classes || has_functions {
        chunks.push(FileChunk {
            description: format!("Add {} implementation", file_name),
            hint: if has_classes { "classes/functions" } else { "functions" }.to_string(),
            line_count: lines.len() - imports_end,
        });
    }

    if chunks.is_empty() {
        chunks.push(FileChunk {
            description: format!("Add {}", file_name),
            hint: format!("python ({} lines)", lines.len()),
            line_count: lines.len(),
        });
    }

    chunks
}

fn parse_rust_chunks(file_path: &str, lines: &[&str]) -> Vec<FileChunk> {
    let mut chunks = Vec::new();
    let file_name = file_path.split('/').last().unwrap_or(file_path);

    let mut uses_end = 0;
    let mut has_structs = false;
    let mut has_impls = false;
    let mut has_functions = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("use ") || trimmed.starts_with("mod ") {
            uses_end = i + 1;
        }
        if trimmed.starts_with("struct ") || trimmed.starts_with("enum ") { has_structs = true; }
        if trimmed.starts_with("impl ") { has_impls = true; }
        if trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ") || trimmed.starts_with("async fn ") { has_functions = true; }
    }

    if uses_end > 0 {
        chunks.push(FileChunk {
            description: format!("Add module imports for {}", file_name),
            hint: "use/mod statements".to_string(),
            line_count: uses_end,
        });
    }

    if has_structs {
        chunks.push(FileChunk {
            description: format!("Add type definitions for {}", file_name),
            hint: "structs/enums".to_string(),
            line_count: (lines.len() - uses_end) / 2,
        });
    }

    if has_impls || has_functions {
        chunks.push(FileChunk {
            description: format!("Add implementations for {}", file_name),
            hint: "impl/functions".to_string(),
            line_count: (lines.len() - uses_end) / 2,
        });
    }

    if chunks.is_empty() {
        chunks.push(FileChunk {
            description: format!("Add {}", file_name),
            hint: format!("rust ({} lines)", lines.len()),
            line_count: lines.len(),
        });
    }

    chunks
}

fn parse_js_chunks(file_path: &str, lines: &[&str]) -> Vec<FileChunk> {
    let mut chunks = Vec::new();
    let file_name = file_path.split('/').last().unwrap_or(file_path);

    let mut imports_end = 0;
    let mut has_components = false;
    let mut has_functions = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") || trimmed.starts_with("const ") && trimmed.contains("require(") {
            imports_end = i + 1;
        }
        if trimmed.contains("function ") || trimmed.contains("const ") && trimmed.contains(" = (") {
            has_functions = true;
        }
        if trimmed.contains("React") || trimmed.contains("Component") || trimmed.starts_with("export default") {
            has_components = true;
        }
    }

    if imports_end > 0 {
        chunks.push(FileChunk {
            description: format!("Add imports for {}", file_name),
            hint: "imports".to_string(),
            line_count: imports_end,
        });
    }

    if has_components || has_functions {
        chunks.push(FileChunk {
            description: format!("Add {} implementation", file_name),
            hint: if has_components { "component" } else { "functions" }.to_string(),
            line_count: lines.len() - imports_end,
        });
    }

    if chunks.is_empty() {
        chunks.push(FileChunk {
            description: format!("Add {}", file_name),
            hint: format!("javascript ({} lines)", lines.len()),
            line_count: lines.len(),
        });
    }

    chunks
}

fn parse_go_chunks(file_path: &str, lines: &[&str]) -> Vec<FileChunk> {
    let mut chunks = Vec::new();
    let file_name = file_path.split('/').last().unwrap_or(file_path);

    let mut imports_end = 0;
    let mut has_types = false;
    let mut has_functions = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") || trimmed == "import (" {
            imports_end = i + 1;
        }
        if trimmed == ")" && imports_end > 0 && i > imports_end {
            imports_end = i + 1;
        }
        if trimmed.starts_with("type ") { has_types = true; }
        if trimmed.starts_with("func ") { has_functions = true; }
    }

    if imports_end > 0 {
        chunks.push(FileChunk {
            description: format!("Add package and imports for {}", file_name),
            hint: "package/imports".to_string(),
            line_count: imports_end,
        });
    }

    if has_types || has_functions {
        chunks.push(FileChunk {
            description: format!("Add {} implementation", file_name),
            hint: if has_types { "types/functions" } else { "functions" }.to_string(),
            line_count: lines.len() - imports_end,
        });
    }

    if chunks.is_empty() {
        chunks.push(FileChunk {
            description: format!("Add {}", file_name),
            hint: format!("go ({} lines)", lines.len()),
            line_count: lines.len(),
        });
    }

    chunks
}

fn parse_ruby_chunks(file_path: &str, lines: &[&str]) -> Vec<FileChunk> {
    let mut chunks = Vec::new();
    let file_name = file_path.split('/').last().unwrap_or(file_path);

    let mut requires_end = 0;
    let mut has_classes = false;
    let mut has_methods = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("require ") || trimmed.starts_with("require_relative ") {
            requires_end = i + 1;
        }
        if trimmed.starts_with("class ") || trimmed.starts_with("module ") { has_classes = true; }
        if trimmed.starts_with("def ") { has_methods = true; }
    }

    if requires_end > 0 {
        chunks.push(FileChunk {
            description: format!("Add requires for {}", file_name),
            hint: "requires".to_string(),
            line_count: requires_end,
        });
    }

    if has_classes || has_methods {
        chunks.push(FileChunk {
            description: format!("Add {} implementation", file_name),
            hint: if has_classes { "class/module" } else { "methods" }.to_string(),
            line_count: lines.len() - requires_end,
        });
    }

    if chunks.is_empty() {
        chunks.push(FileChunk {
            description: format!("Add {}", file_name),
            hint: format!("ruby ({} lines)", lines.len()),
            line_count: lines.len(),
        });
    }

    chunks
}

fn ext_to_type(ext: &str) -> &str {
    match ext {
        "py" => "python",
        "rs" => "rust",
        "js" => "javascript",
        "ts" => "typescript",
        "jsx" | "tsx" => "react",
        "go" => "go",
        "rb" => "ruby",
        "md" => "markdown",
        "json" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        _ => "file",
    }
}

fn get_file_description(file_path: &str, content: &str, ext: &str) -> (String, String) {
    let file_name = file_path.split('/').last().unwrap_or(file_path);
    let line_count = content.lines().count();

    // Check for common patterns
    let is_test = file_path.contains("test") || file_path.contains("spec");
    let is_config = file_name.ends_with(".json") || file_name.ends_with(".toml") ||
                    file_name.ends_with(".yaml") || file_name.ends_with(".yml") ||
                    file_name == "Cargo.toml" || file_name == "package.json";
    let is_docs = file_name.ends_with(".md") || file_path.contains("docs/");

    let desc = if is_test {
        format!("Add tests: {}", file_name)
    } else if is_config {
        format!("Add config: {}", file_name)
    } else if is_docs {
        format!("Add docs: {}", file_name)
    } else {
        format!("Add {}", file_name)
    };

    let hint = format!("{} ({} lines)", ext_to_type(ext), line_count);
    (desc, hint)
}

// File priority for ordering (lower = earlier)
fn file_priority(file: &str) -> u32 {
    let name = file.split('/').last().unwrap_or(file).to_lowercase();
    let path = file.to_lowercase();

    // Config files first
    if name == "cargo.toml" || name == "package.json" || name == "pyproject.toml" || name == "go.mod" {
        return 0;
    }
    if name.ends_with(".toml") || name.ends_with(".json") || name.ends_with(".yaml") || name.ends_with(".yml") {
        return 1;
    }
    // Then utilities/helpers
    if path.contains("util") || path.contains("helper") || path.contains("lib") {
        return 2;
    }
    // Then core/models
    if path.contains("core") || path.contains("model") || path.contains("schema") {
        return 3;
    }
    // Then main features
    if path.contains("service") || path.contains("handler") || path.contains("controller") {
        return 4;
    }
    // Tests later
    if path.contains("test") || path.contains("spec") {
        return 8;
    }
    // Docs last
    if name.ends_with(".md") || path.contains("docs") {
        return 9;
    }
    // Everything else
    5
}

// Diff hunk representation
struct DiffHunk {
    file: String,
    start_line: usize,
    additions: usize,
    deletions: usize,
    description: String,
}

fn parse_diff_hunks(diff: &str) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();
    let mut current_file = String::new();

    for line in diff.lines() {
        if line.starts_with("+++ b/") {
            current_file = line.trim_start_matches("+++ b/").to_string();
        } else if line.starts_with("@@ ") {
            // Parse hunk header: @@ -start,count +start,count @@ context
            let parts: Vec<&str> = line.split("@@").collect();
            if parts.len() >= 2 {
                let range_part = parts[1].trim();
                let context = if parts.len() > 2 { parts[2].trim() } else { "" };

                // Parse +start,count
                let mut start_line = 1;
                let additions = 5; // Simplified - would need to parse hunk content
                let deletions = 2;

                for part in range_part.split_whitespace() {
                    if part.starts_with('+') {
                        let nums: Vec<&str> = part.trim_start_matches('+').split(',').collect();
                        start_line = nums.first().and_then(|s| s.parse().ok()).unwrap_or(1);
                    }
                }

                let desc = if context.is_empty() {
                    format!("Changes at line {}", start_line)
                } else {
                    format!("{}", context)
                };

                hunks.push(DiffHunk {
                    file: current_file.clone(),
                    start_line,
                    additions,
                    deletions,
                    description: desc,
                });
            }
        }
    }

    hunks
}

fn merge_groups_to_target(mut groups: Vec<SplitGroup>, target: usize) -> Vec<SplitGroup> {
    if groups.len() <= target {
        return groups;
    }

    // Simple merge: combine adjacent small groups
    while groups.len() > target {
        // Find smallest adjacent pair to merge
        let mut min_size = usize::MAX;
        let mut merge_idx = 0;

        for i in 0..groups.len() - 1 {
            let combined = groups[i].line_count + groups[i + 1].line_count;
            if combined < min_size {
                min_size = combined;
                merge_idx = i;
            }
        }

        // Merge
        let next = groups.remove(merge_idx + 1);
        groups[merge_idx].files.extend(next.files);
        groups[merge_idx].line_count += next.line_count;
        groups[merge_idx].description = format!("{} + {}", groups[merge_idx].description, next.description);
        groups[merge_idx].hint = format!("{}, {}", groups[merge_idx].hint, next.hint);
    }

    groups
}

fn format_split_suggestion(suggestion: &SplitSuggestion, mode: &str) -> String {
    let mut out = format!("# {} Split Suggestion\n\n", mode.to_uppercase());
    out.push_str(&format!("**{} commit groups** suggested\n\n", suggestion.total_groups));
    out.push_str("## Groups (in suggested order):\n\n");

    for id in &suggestion.suggested_order {
        if let Some(group) = suggestion.groups.iter().find(|g| g.group_id == *id) {
            out.push_str(&format!("### Group {} - {}\n", group.group_id + 1, group.description));
            out.push_str(&format!("- **Files**: {}\n", group.files.join(", ")));
            out.push_str(&format!("- **Hint**: {}\n", group.hint));
            out.push_str(&format!("- **Lines**: ~{}\n\n", group.line_count));
        }
    }

    out.push_str("## Workflow:\n");
    out.push_str("For each group:\n");
    out.push_str("1. `unstage_all` (reset staging)\n");
    out.push_str("2. `stage_files` with the group's files\n");
    out.push_str("3. `get_diff` to see exactly what's staged\n");
    out.push_str("4. Generate a commit message based on the diff\n");
    out.push_str("5. `create_commit` with message (and optional timestamp)\n");

    out
}

// ============================================================================
// Server Info
// ============================================================================

#[tool_handler]
impl ServerHandler for GitBahnServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability::default()),
                ..Default::default()
            },
            server_info: Implementation {
                name: "gitbahn-mcp".to_string(),
                title: Some("gitBahn MCP Server".to_string()),
                version: "0.2.0".to_string(),
                icons: None,
                website_url: Some("https://github.com/0xinit/gitBahn".to_string()),
            },
            instructions: Some(
                "gitBahn provides git operations and smart split suggestions for Claude Code. \
                Use suggest_realistic_split, suggest_atomic_split, or suggest_granular_split \
                to get file groupings, then stage each group and create commits. \
                YOU generate commit messages by analyzing diffs - no API key needed.".to_string()
            ),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = GitBahnServer::new();
    let transport = stdio();
    let server = service.serve(transport).await?;
    // Keep server running until client disconnects
    server.waiting().await?;
    Ok(())
}
