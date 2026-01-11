//! Unit tests for core modules

#[cfg(test)]
mod git_tests {
    use crate::core::git::*;

    #[test]
    fn test_parse_diff_into_hunks_simple() {
        let diff = r#"diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
+    println!("Hello");
     println!("World");
 }
"#;
        let hunks = parse_diff_into_hunks(diff);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].file_path, "src/main.rs");
        assert_eq!(hunks[0].additions, 1);
        assert_eq!(hunks[0].deletions, 0);
        assert!(!hunks[0].is_new_file);
    }

    #[test]
    fn test_parse_diff_into_hunks_new_file() {
        let diff = r#"diff --git a/new_file.rs b/new_file.rs
new file mode 100644
--- /dev/null
+++ b/new_file.rs
@@ -0,0 +1,5 @@
+fn new_function() {
+    println!("New");
+}
+
+const VALUE: i32 = 42;
"#;
        let hunks = parse_diff_into_hunks(diff);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].file_path, "new_file.rs");
        assert!(hunks[0].is_new_file);
        assert_eq!(hunks[0].additions, 5);
    }

    #[test]
    fn test_parse_diff_into_hunks_multiple_hunks() {
        let diff = r#"diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,4 @@
 use std::io;
+use std::fs;

 fn read() {
@@ -10,6 +11,7 @@ fn read() {
 }

 fn write() {
+    // Write data
     let data = "test";
 }
"#;
        let hunks = parse_diff_into_hunks(diff);
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].file_path, "src/lib.rs");
        assert_eq!(hunks[1].file_path, "src/lib.rs");
    }

    #[test]
    fn test_parse_diff_into_hunks_deleted_file() {
        let diff = r#"diff --git a/old_file.rs b/old_file.rs
deleted file mode 100644
--- a/old_file.rs
+++ /dev/null
@@ -1,3 +0,0 @@
-fn old() {
-    println!("Gone");
-}
"#;
        let hunks = parse_diff_into_hunks(diff);
        assert_eq!(hunks.len(), 1);
        assert!(hunks[0].is_deleted);
        assert_eq!(hunks[0].deletions, 3);
    }

    #[test]
    fn test_staged_changes_is_empty() {
        let changes = StagedChanges {
            added: vec![],
            modified: vec![],
            deleted: vec![],
            renamed: vec![],
            diff: String::new(),
            stats: DiffStats::default(),
        };
        assert!(changes.is_empty());

        let changes_with_added = StagedChanges {
            added: vec!["file.rs".to_string()],
            modified: vec![],
            deleted: vec![],
            renamed: vec![],
            diff: String::new(),
            stats: DiffStats::default(),
        };
        assert!(!changes_with_added.is_empty());
    }

    #[test]
    fn test_staged_changes_summary() {
        let changes = StagedChanges {
            added: vec!["a.rs".to_string(), "b.rs".to_string()],
            modified: vec!["c.rs".to_string()],
            deleted: vec![],
            renamed: vec![],
            diff: String::new(),
            stats: DiffStats::default(),
        };
        let summary = changes.summary();
        assert!(summary.contains("2 added"));
        assert!(summary.contains("1 modified"));
    }

    #[test]
    fn test_chunk_type_display() {
        assert_eq!(format!("{}", ChunkType::Imports), "imports");
        assert_eq!(format!("{}", ChunkType::Function), "function");
        assert_eq!(format!("{}", ChunkType::ClassDefinition), "class");
        assert_eq!(format!("{}", ChunkType::FullFile), "full");
    }
}

#[cfg(test)]
mod ai_tests {
    use crate::core::ai::*;

    #[test]
    fn test_code_review_parsing() {
        let json = r#"{
            "verdict": "approve",
            "summary": "Good code",
            "issues": [],
            "positives": ["Clean code"],
            "overall_score": 8
        }"#;
        let parsed: Result<CodeReview, _> = serde_json::from_str(json);
        assert!(parsed.is_ok());
        let review = parsed.unwrap();
        assert_eq!(review.verdict, "approve");
        assert_eq!(review.overall_score, 8);
    }

    #[test]
    fn test_review_issue_parsing() {
        let json = r#"{
            "severity": "warning",
            "file": "main.rs",
            "line": 42,
            "message": "Consider using match",
            "suggestion": "Use match instead of if-else"
        }"#;
        let parsed: Result<ReviewIssue, _> = serde_json::from_str(json);
        assert!(parsed.is_ok());
        let issue = parsed.unwrap();
        assert_eq!(issue.severity, "warning");
        assert_eq!(issue.line, Some(42));
    }
}

#[cfg(test)]
mod config_tests {
    use crate::config::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.ai.model, "claude-sonnet-4-20250514");
        assert!(config.commit.conventional);
        assert!(!config.commit.atomic);
        assert!(!config.commit.sign);
        assert_eq!(config.auto.interval, 30);
        assert_eq!(config.auto.max_commits, 100);
        assert_eq!(config.review.strictness, "normal");
    }

    #[test]
    fn test_ai_config_defaults() {
        let ai = AiConfig::default();
        assert!(ai.anthropic_api_key.is_none());
        assert!(ai.openai_api_key.is_none());
        assert!(ai.elite_coder_url.is_none());
    }

    #[test]
    fn test_commit_config_defaults() {
        let commit = CommitConfig::default();
        assert!(commit.conventional);
        assert!(!commit.atomic);
        assert!(!commit.sign);
        assert!(commit.default_agent.is_none());
        assert!(commit.template.is_none());
    }

    #[test]
    fn test_auto_config_defaults() {
        let auto = AutoConfig::default();
        assert_eq!(auto.interval, 30);
        assert_eq!(auto.max_commits, 100);
        assert!(!auto.rewrite_history);
        assert_eq!(auto.squash_threshold, 5);
        assert!(!auto.auto_push);
    }

    #[test]
    fn test_docs_config_defaults() {
        let docs = DocsConfig::default();
        assert_eq!(docs.format, "auto");
        assert!(!docs.update_existing);
        assert!(docs.exclude.contains(&"node_modules".to_string()));
        assert!(docs.exclude.contains(&"target".to_string()));
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string(&config);
        assert!(toml_str.is_ok());
    }

    #[test]
    fn test_config_deserialization() {
        let toml_str = r#"
[ai]
model = "claude-opus-4-20250514"

[commit]
conventional = false
atomic = true
"#;
        let config: Result<Config, _> = toml::from_str(toml_str);
        assert!(config.is_ok());
        let config = config.unwrap();
        assert_eq!(config.ai.model, "claude-opus-4-20250514");
        assert!(!config.commit.conventional);
        assert!(config.commit.atomic);
    }
}

#[cfg(test)]
mod secrets_tests {
    use crate::core::secrets::*;

    #[test]
    fn test_detect_openai_key() {
        let content = "OPENAI_API_KEY=sk-1234567890abcdefghijklmnop";
        let matches = detect_secrets(content, "config.py");
        assert!(!matches.is_empty());
    }

    #[test]
    fn test_detect_github_token() {
        let content = "token: ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
        let matches = detect_secrets(content, "config.yml");
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|m| m.secret_type.contains("GitHub")));
    }

    #[test]
    fn test_detect_private_key() {
        let content = "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQ...";
        let matches = detect_secrets(content, "key.pem");
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|m| m.secret_type.contains("Private Key")));
    }

    #[test]
    fn test_check_diff_for_secrets() {
        let diff = r#"diff --git a/.env b/.env
--- /dev/null
+++ b/.env
@@ -0,0 +1 @@
+API_KEY=sk-1234567890abcdefghijklmnop
"#;
        let secrets = check_diff_for_secrets(diff);
        assert!(!secrets.is_empty());
    }

    #[test]
    fn test_format_secret_warnings() {
        let secrets = vec![SecretMatch {
            secret_type: "OpenAI API Key".to_string(),
            line: 1,
            masked_value: "sk-12...mnop".to_string(),
            confidence: 0.95,
            file_path: "config.py".to_string(),
        }];
        let output = format_secret_warnings(&secrets);
        assert!(output.contains("POTENTIAL SECRETS DETECTED"));
        assert!(output.contains("OpenAI API Key"));
    }
}