# gitBahn

Autonomous Git operations with AI-powered commits, merges, and code rewriting.

## Why gitBahn?

AI coding assistants can write entire projects in minutes. But when you commit that code, the git history tells a different story:

```
commit abc123  2025-01-10 14:32:01  feat: add complete trading bot
  28 files changed, 5278 insertions(+)
```

**One massive commit. All files at once. Obviously AI-generated.**

gitBahn transforms this into a realistic development history:

```
commit 9fd434c  Jan 03, 11:17  chore: initialize project with gitignore
commit 57cb30b  Jan 03, 13:12  feat: add environment configuration
commit 97096e8  Jan 03, 15:13  feat(utils): add shared constants
commit 2be67ce  Jan 03, 16:08  feat(storage): add data models
commit fd82b57  Jan 03, 17:56  feat(storage): implement Redis client
...47 commits over 2 days...
commit a504534  Jan 05, 02:23  docs: add comprehensive README
```

**Small focused commits. Spread over time. Natural development progression.**

### What gitBahn Offers

| Feature | Standard Git | gitBahn |
|---------|--------------|---------|
| Custom timestamps | Limited | Full control |
| Spread commits over time | Manual | Automatic |
| Split 1 change → N commits | Manual | AI-powered |
| Split files by function | Not possible | Automatic |
| Realistic development flow | Manual | `--realistic` mode |

### Perfect For

- Portfolio projects that showcase your work
- Open source contributions with clean history
- Teams that want organized, readable git logs
- Anyone using AI assistants who wants natural-looking commits

## Features

- **AI Commit**: Generate intelligent commit messages from diffs
- **Auto Mode**: Watch for changes and auto-commit with AI messages
- **Code Rewrite**: Transform code with natural language instructions
- **AI Merge**: Resolve merge conflicts automatically with AI
- **Code Review**: Get AI-powered code reviews
- **Docs Generation**: Generate documentation for your code

## Installation

```bash
cargo install --path .
```

## Configuration

Set your Anthropic API key:

```bash
export ANTHROPIC_API_KEY=your_key_here
```

Or initialize with a config file:

```bash
bahn init
```

## Usage

### Commit with AI

```bash
# Generate AI commit message for staged changes
bahn commit

# Split into atomic commits
bahn commit --atomic

# Atomic commits with spread timestamps (human-like)
bahn commit --atomic --spread 4h

# Atomic commits with custom start time
bahn commit --atomic --spread 4h --start "2025-01-05 09:00"

# Split into exactly N commits
bahn commit --atomic --split 10

# Granular mode - split files into hunks for ultra-realistic commits
bahn commit --granular --spread 4h

# Granular with exact commit count
bahn commit -g --split 15 --spread 2h --start "2025-01-05 09:00"

# Auto-confirm without prompting
bahn commit -y
```

### Autonomous Mode

```bash
# Watch and auto-commit changes (real-time)
bahn auto --watch

# Custom interval (seconds)
bahn auto --watch --interval 60

# Dry run - see what would be committed
bahn auto --dry-run
```

### Human-like Commits (Stealth Mode)

```bash
# Interactive mode - prompt before each commit with timestamp choice
bahn auto --watch --prompt

# Deferred mode - collect commits, spread timestamps on exit
bahn auto --watch --defer --spread 4h

# Deferred with custom start time
bahn auto --watch --defer --spread 4h --start "2025-01-05 09:00"
```

**`--prompt` mode** asks you for each change:
- Commit now (current time)
- Commit with backdated time (e.g., "2h ago")
- Add to batch (commit later with spread timestamps)
- Skip

**`--defer` mode** collects all commits during your session, then creates them with randomly spread timestamps when you press Ctrl+C. Perfect for making AI-assisted coding look natural.

### Granular Commits (Ultra-Realistic)

```bash
# Split individual files into hunks (chunks) for realistic history
bahn commit --granular

# Combine with spread timestamps
bahn commit -g --spread 4h --start "2025-01-05 09:00"

# Request specific number of commits
bahn commit -g --split 20 --spread 6h -y
```

**`--granular` mode** analyzes your changes at the hunk level (individual chunks within files) rather than whole files. This creates commits that look like natural, incremental development:

- A single file can be split across multiple commits
- Related hunks across files are grouped together
- Earlier commits contain foundational code (imports, types)
- Later commits build on earlier ones (implementations)
- Each commit is self-contained and won't break the build

### Realistic Mode (Maximum Authenticity)

```bash
# Simulate human development flow
bahn commit --realistic

# Target specific commit count spread over time
bahn commit --realistic --split 47 --spread 48h --start "2025-01-03 11:17"

# Short form with auto-confirm
bahn commit -r --split 30 --spread 24h -y
```

**`--realistic` mode** is the most sophisticated option. It simulates how a human developer actually builds a project:

1. **Language-aware parsing** - Understands Python, Rust, JavaScript/TypeScript, and Go
2. **Logical chunking** - Splits files into imports, constants, classes, and individual methods
3. **Dependency ordering** - Config files first, then utils, models, services, and finally entry points
4. **Progressive building** - Large files grow across multiple commits (imports → class skeleton → methods)
5. **Natural interleaving** - Work on module A, switch to B, come back to A

**Commit Mode Comparison:**

| Mode | Splits by | Best for |
|------|-----------|----------|
| `--atomic` | Whole files | Quick splitting |
| `--granular` | Hunks (diff chunks) | Modified files |
| `--realistic` | Logical code units | New projects, maximum authenticity |

### Code Rewrite

```bash
# Rewrite a file with AI
bahn rewrite src/main.rs --instructions "Add error handling"

# Rewrite entire directory
bahn rewrite src/ --instructions "Convert to async"

# Dry run
bahn rewrite src/main.rs --dry-run
```

### AI Merge

```bash
# Merge with AI conflict resolution
bahn merge feature-branch --auto-resolve
```

### Code Review

```bash
# Review staged changes
bahn review --staged

# Review specific commit
bahn review --commit abc123

# Strict review
bahn review --staged --strictness strict
```

### Documentation

```bash
# Generate docs for a file
bahn docs src/main.rs

# Specify format
bahn docs src/lib.rs --format markdown
```

### Status

```bash
# Show repository status
bahn status
```

## Claude Code Plugin (Recommended)

Use gitBahn as a Claude Code plugin - **no API key needed**. Claude Code handles all AI operations directly.

### Install as Plugin

```bash
claude plugin add https://github.com/0xinit/gitBahn
```

Or manually:
```bash
cd ~/.claude/plugins
git clone https://github.com/0xinit/gitBahn.git gitbahn
cd gitbahn/gitbahn-mcp
cargo build --release
```

### Usage

Just talk to Claude Code naturally:

```
"Commit my changes"
"Create realistic commits spread over 4 hours"
"Split my changes into atomic commits"
"Create 10 commits spread over 2 days starting yesterday at 9am"
```

### Available Tools

**Git Operations:**
- `get_status`, `get_diff`, `list_changes`
- `stage_all`, `stage_files`, `unstage_all`
- `create_commit` (with optional timestamp for backdating)
- `get_log`, `get_branch`, `push`, `undo`

**Smart Split Suggestions:**
- `suggest_realistic_split` - Language-aware splitting (imports → classes → functions)
- `suggest_atomic_split` - One file per commit
- `suggest_granular_split` - Split by diff hunks

### How It Works

When you ask for realistic commits, Claude Code:
1. Calls `suggest_realistic_split` → gets file groupings
2. For each group: stages files, analyzes diff, generates commit message
3. Calls `create_commit` with message and timestamp

**No Anthropic API key needed** - Claude Code does the AI work directly.

### Manual MCP Configuration

If not using as a plugin, add to your `.mcp.json`:

```json
{
  "mcpServers": {
    "gitbahn": {
      "command": "gitbahn-mcp"
    }
  }
}
```

## License

MIT
