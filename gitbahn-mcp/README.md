# gitBahn MCP Server

Model Context Protocol (MCP) server for gitBahn - enables AI assistants like Claude to create realistic git commits.

## Installation

```bash
cd gitbahn-mcp
cargo install --path .
```

## Configuration

### Claude Code

Add to `~/.claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "gitbahn": {
      "command": "gitbahn-mcp"
    }
  }
}
```

### Claude Desktop

Add to your Claude Desktop settings:

```json
{
  "mcpServers": {
    "gitbahn": {
      "command": "gitbahn-mcp"
    }
  }
}
```

## Available Tools

### realistic_commit

Creates commits that simulate human development flow. Splits files into logical chunks (imports, classes, methods) and commits them progressively over time.

**Parameters:**
- `split` - Number of commits to create (e.g., 30)
- `spread` - Duration to spread commits over (e.g., "24h", "48h", "7d")
- `start` - Start timestamp (e.g., "2025-01-03 11:17")
- `auto_confirm` - Skip prompts (default: true)

**Best for:** New projects where you want maximum authenticity.

### atomic_commit

Creates atomic commits split by file. Each changed file gets its own commit with an AI-generated message.

**Parameters:** Same as realistic_commit

**Best for:** Quick splitting of changes.

### granular_commit

Creates granular commits split by hunks (diff chunks within files). Allows splitting a single file across multiple commits.

**Parameters:** Same as realistic_commit

**Best for:** Modified files where you want fine-grained history.

### simple_commit

Creates a single commit with an AI-generated message for all staged changes.

**Parameters:**
- `auto_confirm` - Skip prompts (default: true)

### stage_all

Stages all changes in the repository (`git add -A`).

### git_status

Shows staged and unstaged changes.

## Example Usage

Once configured, Claude can use gitBahn tools directly:

> "Stage all my changes and create 30 realistic commits spread over the past 2 days"

Claude will call:
1. `stage_all`
2. `realistic_commit` with `split=30`, `spread="48h"`, `start="2025-01-08 09:00"`

## Requirements

- gitBahn CLI must be installed (`cargo install --path /path/to/gitBahn`)
- `ANTHROPIC_API_KEY` environment variable set