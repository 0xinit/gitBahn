# gitBahn

Autonomous Git operations with AI-powered commits, merges, and code rewriting.

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

## License

MIT
