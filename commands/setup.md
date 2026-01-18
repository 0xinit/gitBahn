---
description: Set up gitBahn MCP server
---

# gitBahn Setup

Install the gitBahn MCP server for git operations in Claude Code.

## Quick Install

```bash
cd ${CLAUDE_PLUGIN_ROOT}/gitbahn-mcp
cargo install --path .
```

## Verify Installation

```bash
which gitbahn-mcp
```

## No API Key Required

The MCP server doesn't need an Anthropic API key. Claude Code handles all AI operations directly by analyzing diffs.

## Available Tools

### Git Operations
| Tool | Description |
|------|-------------|
| `get_status` | Show staged/unstaged changes |
| `get_diff` | Get diff of changes |
| `list_changes` | List files grouped by status |
| `stage_all` | Stage all changes |
| `stage_files` | Stage specific files |
| `unstage_all` | Unstage all files |
| `create_commit` | Create commit with message + optional timestamp |
| `get_log` | Show commit history |
| `get_branch` | Show current branch |
| `push` | Push to remote |
| `undo` | Undo recent commits |

### Smart Split Suggestions
| Tool | Description |
|------|-------------|
| `suggest_realistic_split` | Split by language constructs (imports, classes, functions) |
| `suggest_atomic_split` | Split by file (one commit per file) |
| `suggest_granular_split` | Split by diff hunks (changes within files) |

## Example Workflows

### Simple commit
```
"Commit my changes"
→ stage_all → get_diff → [analyze] → create_commit
```

### Realistic commits
```
"Create realistic commits spread over 4 hours"
→ stage_all → suggest_realistic_split → [loop: unstage_all, stage_files, get_diff, create_commit with timestamp]
```

### Atomic commits
```
"Create one commit per file"
→ stage_all → suggest_atomic_split → [loop: unstage_all, stage_files, get_diff, create_commit]
```
