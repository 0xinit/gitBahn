---
description: Create a commit with AI-generated message (no API key needed)
---

# Smart Commit

Create commits with intelligent commit messages - Claude Code generates the message by analyzing the diff.

## Instructions

1. **Get the diff** using `get_diff` tool (staged changes by default)
2. **Analyze the changes** and generate a conventional commit message:
   - Use format: `type(scope): description`
   - Types: feat, fix, docs, style, refactor, test, chore
   - Keep the first line under 72 characters
   - Add body for complex changes
3. **Create the commit** using `create_commit` tool with your message

## Example Flow

```
User: "commit my changes"

1. Call get_status to see what's changed
2. Call stage_all if needed
3. Call get_diff to see the actual changes
4. Analyze the diff and generate a message like:
   "feat(auth): add JWT token validation middleware"
5. Call create_commit with the message
```

## Commit Message Guidelines

- **feat**: New feature
- **fix**: Bug fix
- **docs**: Documentation only
- **style**: Formatting, no code change
- **refactor**: Code restructuring
- **test**: Adding tests
- **chore**: Maintenance tasks

## No API Key Required

Unlike the standalone CLI, when using gitBahn through Claude Code, no Anthropic API key is needed. Claude Code itself analyzes the diff and generates the commit message.
