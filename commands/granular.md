---
description: Create granular commits - split by diff hunks
---

# Granular Commits

Split by diff hunks - allows splitting a single file across multiple commits. Best for modified files with multiple logical changes.

## Quick Start

1. Stage changes: `stage_all`
2. Get split suggestion: `suggest_granular_split`
3. Create commits for each hunk group

## How It Works

The `suggest_granular_split` tool:
- Parses the staged diff into individual hunks
- Each hunk represents a contiguous block of changes
- Shows the function/context where changes occur
- Allows fine-grained control over commits

## Workflow

```
1. stage_all
2. suggest_granular_split
3. For each hunk group:
   a. unstage_all
   b. stage_files [affected files]  # Note: stages whole file
   c. get_diff
   d. [Generate commit message]
   e. create_commit {message}
```

## Example Output

```
# GRANULAR Split Suggestion

**4 commit groups** suggested

### Group 1 - fn validate_user
- Files: src/auth.rs
- Hint: src/auth.rs:45 (+12/-3)

### Group 2 - fn hash_password
- Files: src/auth.rs
- Hint: src/auth.rs:120 (+8/-2)

### Group 3 - struct Config
- Files: src/config.rs
- Hint: src/config.rs:10 (+15/-0)

### Group 4 - fn load_config
- Files: src/config.rs
- Hint: src/config.rs:50 (+20/-5)
```

## Options

- `target_commits`: Merge hunks to hit a specific number

## When to Use

- Modified files with multiple distinct changes
- When you want to separate unrelated changes in the same file
- Creating a clean history from a large refactoring

## Limitation

Currently stages whole files rather than individual hunks. For true hunk-level staging, use the standalone `bahn` CLI with `--granular` flag.
