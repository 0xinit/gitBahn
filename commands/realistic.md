---
description: Create realistic commits that simulate human development flow
---

# Realistic Commits

Create commits that look like natural human development.

## Instructions

Use the `realistic_commit` MCP tool to create commits that simulate how a developer actually builds a project:

1. **Stage changes first** using `stage_all` tool
2. **Create realistic commits** using `realistic_commit` tool with:
   - `split`: Number of commits (e.g., 30)
   - `spread`: Time duration (e.g., "24h", "48h", "7d")
   - `start`: Start timestamp (e.g., "2025-01-03 11:17")

## Example

To create 30 commits spread over 2 days starting yesterday at 9am:

1. Call `stage_all` to stage all changes
2. Call `realistic_commit` with:
   - split: 30
   - spread: "48h"
   - start: "[yesterday's date] 09:00"

## How It Works

Realistic mode:
- Parses files by language (Python, Rust, JS/TS, Go)
- Splits into logical chunks (imports, classes, methods)
- Orders by dependency (config → utils → models → services)
- Interleaves work across files naturally
- Spreads timestamps realistically
