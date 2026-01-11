---
description: Install gitBahn CLI and MCP server
---

# gitBahn Setup

Install the gitBahn CLI and MCP server for realistic git commits.

## Instructions

1. First, check if cargo (Rust) is installed:
```bash
cargo --version
```

2. If cargo is not installed, install Rust first:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

3. Install the gitBahn CLI:
```bash
cd ${CLAUDE_PLUGIN_ROOT}
cargo install --path .
```

4. Install the MCP server:
```bash
cd ${CLAUDE_PLUGIN_ROOT}/gitbahn-mcp
cargo install --path .
```

5. Set your Anthropic API key (required for AI commit messages):
```bash
export ANTHROPIC_API_KEY=your_key_here
```

6. Verify installation:
```bash
bahn --version
which gitbahn-mcp
```

## After Setup

The gitBahn tools will be available:
- `realistic_commit` - Human-like development flow
- `atomic_commit` - Split by files
- `granular_commit` - Split by hunks
- `simple_commit` - Single AI commit
- `stage_all` - Stage all changes
- `git_status` - Show changes

Try: "Stage all changes and create 10 realistic commits spread over 4 hours"