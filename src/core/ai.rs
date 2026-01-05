//! AI integration for commit message generation and code review.

use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Retry configuration for API calls
const MAX_RETRIES: u32 = 3;
const BASE_DELAY_MS: u64 = 1000;
const MAX_DELAY_MS: u64 = 30000;

/// Message for the Claude API
#[derive(Debug, Serialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// Request to Claude API
#[derive(Debug, Serialize)]
pub struct ClaudeRequest {
    pub model: String,
    pub max_tokens: u32,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
}

/// Response from Claude API
#[derive(Debug, Deserialize)]
pub struct ClaudeResponse {
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
pub struct ContentBlock {
    pub text: String,
}

/// AI client for interacting with Claude
pub struct AiClient {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl AiClient {
    /// Create a new AI client
    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model: model.unwrap_or_else(|| "claude-sonnet-4-20250514".to_string()),
        }
    }

    /// Generate a commit message from a diff
    pub async fn generate_commit_message(
        &self,
        diff: &str,
        context: Option<&str>,
        personality: Option<&str>,
    ) -> Result<String> {
        let system_prompt = self.build_commit_system_prompt(personality);

        let mut user_content = String::new();
        user_content.push_str("Generate a commit message for the following changes:\n\n");

        if let Some(ctx) = context {
            user_content.push_str(&format!("Context: {}\n\n", ctx));
        }

        user_content.push_str("```diff\n");
        // Truncate diff if too long
        let max_diff_len = 10000;
        if diff.len() > max_diff_len {
            user_content.push_str(&diff[..max_diff_len]);
            user_content.push_str("\n... (truncated)\n");
        } else {
            user_content.push_str(diff);
        }
        user_content.push_str("\n```");

        let response = self.send_message(&system_prompt, &user_content).await?;

        Ok(response.trim().to_string())
    }

    /// Generate multiple atomic commit suggestions
    /// If `target_count` is provided, try to create approximately that many commits
    pub async fn suggest_atomic_commits(
        &self,
        diff: &str,
        files: &[&str],
        target_count: Option<usize>,
    ) -> Result<Vec<AtomicCommitSuggestion>> {
        let target_instruction = if let Some(count) = target_count {
            format!(
                "\n\nIMPORTANT: Split the changes into EXACTLY {} commits. \
                Distribute the files and changes evenly across {} commits. \
                Each commit should represent a logical step in the development process.",
                count, count
            )
        } else {
            String::new()
        };

        let system_prompt = format!(
            r#"You are an expert at analyzing code changes and suggesting atomic commits.

Your task is to analyze a diff and suggest how to split it into atomic commits.
Each atomic commit should:
1. Do exactly one thing
2. Be self-contained and not break the build
3. Have a clear, conventional commit message
4. Have a UNIQUE message - never repeat the same commit message{}

Respond in JSON format:
{{
  "commits": [
    {{
      "message": "feat(auth): add login validation",
      "files": ["src/auth.rs", "src/validation.rs"],
      "description": "Brief explanation of what this commit does"
    }}
  ]
}}

If the changes should be a single commit, return just one item in the array."#,
            target_instruction
        );

        let mut user_content = String::new();
        user_content.push_str(&format!("Files changed: {}\n\n", files.join(", ")));
        user_content.push_str("```diff\n");

        let max_diff_len = 10000;
        if diff.len() > max_diff_len {
            user_content.push_str(&diff[..max_diff_len]);
            user_content.push_str("\n... (truncated)\n");
        } else {
            user_content.push_str(diff);
        }
        user_content.push_str("\n```");

        let response = self.send_message(&system_prompt, &user_content).await?;

        // Parse JSON response - extract JSON if wrapped in text/markdown
        let json_str = extract_json(&response);
        let parsed: AtomicCommitsResponse = serde_json::from_str(json_str)
            .with_context(|| format!("Failed to parse AI response as JSON: {}", &response[..200.min(response.len())]))?;

        Ok(parsed.commits)
    }

    /// Generate granular commit suggestions based on individual hunks
    /// This allows splitting single files across multiple commits for more realistic history
    pub async fn suggest_granular_commits(
        &self,
        hunks: &[HunkInfo],
        target_count: Option<usize>,
    ) -> Result<Vec<GranularCommitSuggestion>> {
        let target_instruction = if let Some(count) = target_count {
            format!(
                "\n\nIMPORTANT: Create EXACTLY {} commits. \
                Distribute hunks evenly across {} commits, grouping related changes together. \
                Each commit should represent a realistic incremental step in development.",
                count, count
            )
        } else {
            String::new()
        };

        let system_prompt = format!(
            r#"You are an expert at analyzing code changes and creating realistic commit history.

You are given a list of "hunks" - individual chunks of changes within files.
Your task is to group these hunks into commits that look like natural, incremental development.

Rules:
1. Group related hunks together (same feature, same logical change)
2. A single file CAN be split across multiple commits if it has unrelated changes
3. Earlier commits should be foundational (imports, types, struct definitions)
4. Later commits should build on earlier ones (implementations, tests)
5. Each commit should compile on its own (don't separate a function signature from its body)
6. Make it look like a human developed this incrementally, not all at once
7. Each commit message must be UNIQUE - never repeat messages{}

Respond in JSON format:
{{
  "commits": [
    {{
      "message": "feat(auth): add User struct and types",
      "hunk_ids": [0, 2, 5],
      "description": "Foundation for user authentication"
    }}
  ]
}}

The hunk_ids are the IDs provided in the hunk list. Each hunk should appear in exactly one commit."#,
            target_instruction
        );

        // Build a compact representation of hunks for the AI
        let mut user_content = String::new();
        user_content.push_str("Hunks to organize into commits:\n\n");

        for hunk in hunks {
            user_content.push_str(&format!(
                "Hunk {} ({}{}):\n  File: {}\n  Changes: +{} -{}\n  Context: {}\n  Content preview: {}\n\n",
                hunk.id,
                if hunk.is_new_file { "NEW " } else { "" },
                if hunk.is_deleted { "DELETED " } else { "" },
                hunk.file_path,
                hunk.additions,
                hunk.deletions,
                hunk.context,
                hunk.content_preview
            ));
        }

        let response = self.send_message(&system_prompt, &user_content).await?;

        let json_str = extract_json(&response);
        let parsed: GranularCommitsResponse = serde_json::from_str(json_str)
            .with_context(|| format!("Failed to parse granular commits response: {}", &response[..200.min(response.len())]))?;

        Ok(parsed.commits)
    }

    /// Generate documentation for code
    pub async fn generate_docs(
        &self,
        code: &str,
        language: &str,
        format: &str,
    ) -> Result<String> {
        let system_prompt = format!(
            r#"You are an expert at writing clear, concise documentation.

Generate {} format documentation for the following {} code.
Include:
- Brief description of what the code does
- Parameters/arguments (if applicable)
- Return values (if applicable)
- Examples where helpful

Only output the documentation, ready to be inserted into the code."#,
            format, language
        );

        let user_content = format!("```{}\n{}\n```", language, code);

        self.send_message(&system_prompt, &user_content).await
    }

    /// Review code changes
    pub async fn review_code(
        &self,
        diff: &str,
        context: Option<&str>,
        personality: Option<&str>,
        strictness: &str,
    ) -> Result<CodeReview> {
        let system_prompt = self.build_review_system_prompt(personality, strictness);

        let mut user_content = String::new();
        user_content.push_str("Review the following code changes:\n\n");

        if let Some(ctx) = context {
            user_content.push_str(&format!("Context: {}\n\n", ctx));
        }

        user_content.push_str("```diff\n");
        let max_diff_len = 15000;
        if diff.len() > max_diff_len {
            user_content.push_str(&diff[..max_diff_len]);
            user_content.push_str("\n... (truncated)\n");
        } else {
            user_content.push_str(diff);
        }
        user_content.push_str("\n```");

        user_content.push_str("\n\nProvide your review in JSON format with the following structure:\n");
        user_content.push_str(r#"{
  "verdict": "approve" | "request_changes" | "comment",
  "summary": "Brief overall assessment",
  "issues": [
    {
      "severity": "critical" | "warning" | "suggestion",
      "file": "path/to/file",
      "line": 42,
      "message": "Description of the issue",
      "suggestion": "Optional suggested fix"
    }
  ],
  "positives": ["Things done well"],
  "overall_score": 1-10
}"#);

        let response = self.send_message(&system_prompt, &user_content).await?;

        let json_str = extract_json(&response);
        let review: CodeReview = serde_json::from_str(json_str)
            .with_context(|| format!("Failed to parse review response as JSON: {}", &response[..200.min(response.len())]))?;

        Ok(review)
    }

    /// Send a message to Claude API with retry logic
    async fn send_message(&self, system: &str, user: &str) -> Result<String> {
        let request = ClaudeRequest {
            model: self.model.clone(),
            max_tokens: 4096,
            messages: vec![Message {
                role: "user".to_string(),
                content: user.to_string(),
            }],
            system: Some(system.to_string()),
        };

        let mut last_error = None;
        let mut delay_ms = BASE_DELAY_MS;

        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                eprintln!("Retrying API request (attempt {}/{})", attempt + 1, MAX_RETRIES + 1);
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                delay_ms = (delay_ms * 2).min(MAX_DELAY_MS);
            }

            let response = match self.client
                .post("https://api.anthropic.com/v1/messages")
                .header("Content-Type", "application/json")
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&request)
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    // Network errors are retryable
                    last_error = Some(format!("Network error: {}", e));
                    continue;
                }
            };

            let status = response.status();

            // Success - return the response
            if status.is_success() {
                let claude_response: ClaudeResponse = response.json().await
                    .context("Failed to parse Claude API response")?;

                return Ok(claude_response.content
                    .first()
                    .map(|c| c.text.clone())
                    .unwrap_or_default());
            }

            // Check if error is retryable
            let error_text = response.text().await.unwrap_or_default();

            if status.as_u16() == 429 || status.as_u16() >= 500 {
                // Rate limit (429) or server errors (5xx) are retryable
                last_error = Some(format!("API error ({}): {}", status, error_text));
                continue;
            }

            // Non-retryable errors (400, 401, 403, etc.) - fail immediately
            anyhow::bail!("Claude API error ({}): {}", status, error_text);
        }

        // All retries exhausted
        anyhow::bail!("Claude API request failed after {} attempts. Last error: {}",
            MAX_RETRIES + 1,
            last_error.unwrap_or_else(|| "Unknown error".to_string())
        )
    }

    /// Build system prompt for commit messages
    fn build_commit_system_prompt(&self, personality: Option<&str>) -> String {
        let base = r#"You are an expert at writing clear, concise git commit messages.

Follow the Conventional Commits specification:
- Format: <type>(<scope>): <description>
- Types: feat, fix, docs, style, refactor, test, chore, perf, ci, build
- Keep the first line under 72 characters
- Use imperative mood ("add" not "added")
- Focus on WHY, not just WHAT

Output ONLY the commit message, nothing else."#;

        if let Some(p) = personality {
            format!("{}\n\nPersonality: {}", base, p)
        } else {
            base.to_string()
        }
    }

    /// Build system prompt for code reviews
    fn build_review_system_prompt(&self, personality: Option<&str>, strictness: &str) -> String {
        let strictness_desc = match strictness {
            "relaxed" => "Focus on critical issues only. Be lenient on style preferences.",
            "strict" => "Be thorough and strict. Flag all issues including minor style violations.",
            _ => "Balance between thoroughness and pragmatism. Focus on important issues.",
        };

        let base = format!(
            r#"You are an expert code reviewer.

Review Style: {}

Focus on:
- Correctness and potential bugs
- Security vulnerabilities
- Performance issues
- Code clarity and maintainability
- Best practices for the language/framework

Be constructive and specific. Provide actionable feedback."#,
            strictness_desc
        );

        if let Some(p) = personality {
            format!("{}\n\nPersonality: {}", base, p)
        } else {
            base
        }
    }

    /// Rewrite code with AI
    pub async fn rewrite_code(
        &self,
        code: &str,
        language: &str,
        instructions: &str,
    ) -> Result<String> {
        let system_prompt = format!(
            r#"You are an expert {} programmer. Rewrite the following code according to the instructions.

Instructions: {}

Output ONLY the rewritten code, nothing else. No explanations, no markdown code blocks."#,
            language, instructions
        );

        self.send_message(&system_prompt, code).await
    }

    /// Resolve merge conflict with AI
    pub async fn resolve_conflict(
        &self,
        ancestor: &str,
        ours: &str,
        theirs: &str,
    ) -> Result<String> {
        let system_prompt = r#"You are an expert at resolving git merge conflicts.
Given the ancestor version, our version, and their version, produce a merged result.
Combine both sets of changes intelligently, preserving the intent of both sides.

Output ONLY the resolved code, nothing else."#;

        let user_content = format!(
            "=== ANCESTOR ===\n{}\n\n=== OURS ===\n{}\n\n=== THEIRS ===\n{}",
            ancestor, ours, theirs
        );

        self.send_message(system_prompt, &user_content).await
    }

    /// Generate a squash commit message from multiple commits
    pub async fn generate_squash_message(&self, commits_text: &str) -> Result<String> {
        let system_prompt = r#"You are an expert at writing clear, concise git commit messages.

Given multiple commit messages, create a single unified commit message that:
1. Summarizes all the changes in one coherent message
2. Follows Conventional Commits format: <type>(<scope>): <description>
3. Keeps the first line under 72 characters
4. Uses imperative mood
5. Captures the overall intent of all commits

Output ONLY the commit message, nothing else."#;

        let user_content = format!(
            "Summarize these commits into one message:\n\n{}",
            commits_text
        );

        let response = self.send_message(system_prompt, &user_content).await?;
        Ok(response.trim().to_string())
    }
}

/// Suggestion for an atomic commit
#[derive(Debug, Deserialize)]
pub struct AtomicCommitSuggestion {
    pub message: String,
    pub files: Vec<String>,
    pub description: String,
}

#[derive(Debug, Deserialize)]
struct AtomicCommitsResponse {
    commits: Vec<AtomicCommitSuggestion>,
}

/// Simplified hunk info for AI analysis
#[derive(Debug)]
pub struct HunkInfo {
    pub id: usize,
    pub file_path: String,
    pub is_new_file: bool,
    pub is_deleted: bool,
    pub additions: usize,
    pub deletions: usize,
    pub context: String,
    pub content_preview: String,
}

/// Suggestion for a granular commit (hunk-level)
#[derive(Debug, Deserialize)]
pub struct GranularCommitSuggestion {
    pub message: String,
    pub hunk_ids: Vec<usize>,
    pub description: String,
}

#[derive(Debug, Deserialize)]
struct GranularCommitsResponse {
    commits: Vec<GranularCommitSuggestion>,
}

/// Code review result
#[derive(Debug, Serialize, Deserialize)]
pub struct CodeReview {
    pub verdict: String,
    pub summary: String,
    pub issues: Vec<ReviewIssue>,
    pub positives: Vec<String>,
    pub overall_score: u8,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReviewIssue {
    pub severity: String,
    pub file: String,
    pub line: Option<u32>,
    pub message: String,
    pub suggestion: Option<String>,
}

/// Extract JSON from a response that might be wrapped in markdown or text
fn extract_json(response: &str) -> &str {
    let response = response.trim();

    // If it starts with {, assume it's already JSON
    if response.starts_with('{') {
        return response;
    }

    // Try to find JSON in markdown code blocks
    if let Some(start) = response.find("```json") {
        let json_start = start + 7; // Skip ```json
        if let Some(end) = response[json_start..].find("```") {
            return response[json_start..json_start + end].trim();
        }
    }

    // Try plain code blocks
    if let Some(start) = response.find("```") {
        let block_start = start + 3;
        // Skip optional language identifier
        let content_start = response[block_start..]
            .find('\n')
            .map(|i| block_start + i + 1)
            .unwrap_or(block_start);
        if let Some(end) = response[content_start..].find("```") {
            return response[content_start..content_start + end].trim();
        }
    }

    // Try to find { and } and extract
    if let Some(start) = response.find('{') {
        if let Some(end) = response.rfind('}') {
            if end > start {
                return &response[start..=end];
            }
        }
    }

    // Return as-is and let the parser fail with a better error
    response
}
