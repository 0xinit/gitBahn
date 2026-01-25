---
description: Create a commit with AI-generated message
---

# Smart Commit

Create commits with intelligent commit messages - you analyze the diff and generate the message.

## How to Use

When the user asks to "commit" their changes:

### Step 1: Check Status

```bash
git status --short
```

### Step 2: Stage Changes (if needed)

```bash
git add -A
```

### Step 3: Get the Diff

```bash
git diff --cached
```

### Step 4: Generate Commit Message

Analyze the diff and create a conventional commit message:
- Format: `type(scope): description`
- Keep first line under 72 characters
- Add body for complex changes

**Types:**
- `feat` - New feature
- `fix` - Bug fix
- `docs` - Documentation only
- `style` - Formatting, no code change
- `refactor` - Code restructuring
- `test` - Adding tests
- `chore` - Maintenance tasks

### Step 5: Create the Commit

```bash
git commit -m "feat(auth): add JWT token validation middleware"
```

Or with a body:
```bash
git commit -m "feat(auth): add JWT token validation

- Add middleware for validating JWT tokens
- Include refresh token logic
- Handle token expiration gracefully"
```

## With Custom Timestamp

To backdate a commit:

```bash
GIT_AUTHOR_DATE="2025-01-03 14:32:17 +0000" GIT_COMMITTER_DATE="2025-01-03 14:32:17 +0000" git commit -m "message"
```

## Example

User: "commit my changes"

1. Run `git status --short` → sees modified auth.js
2. Run `git add -A`
3. Run `git diff --cached` → sees JWT validation code added
4. Generate message: "feat(auth): add JWT token validation middleware"
5. Run `git commit -m "feat(auth): add JWT token validation middleware"`

## Split Options

For more control over commits, use:
- `/gitbahn:atomic` - One file per commit
- `/gitbahn:realistic` - Language-aware splitting
- `/gitbahn:granular` - Split by diff hunks
