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

Unlike the standalone CLI, the MCP server doesn't need an Anthropic API key. Claude Code handles all AI operations directly.

## Available Tools

After setup, these tools are available:

| Tool | Description |
|------|-------------|
| `get_status` | Show staged/unstaged changes |
| `get_diff` | Get diff of changes |
| `list_changes` | List files by status |
| `stage_all` | Stage all changes |
| `stage_files` | Stage specific files |
| `create_commit` | Create commit with message |
| `get_log` | Show commit history |
| `get_branch` | Show current branch |
| `push` | Push to remote |
| `undo` | Undo recent commits |

## Example Usage

"Stage all my changes and commit them" - Claude Code will:
1. Call `stage_all`
2. Call `get_diff` to see changes
3. Analyze and generate commit message
4. Call `create_commit` with the message

"Create 10 realistic commits spread over 4 hours" - Claude Code will:
1. Analyze the diff
2. Split into logical chunks
3. Create commits with spread timestamps
