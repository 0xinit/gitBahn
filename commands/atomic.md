---
description: Create atomic commits - one file per commit
---

# Atomic Commits

Split changes so each file becomes its own commit. Simple and quick.

## How to Use

When the user asks for "atomic commits", follow this workflow:

### Step 1: Get Changed Files

```bash
git status --porcelain
git add -A
git diff --cached --name-only
```

### Step 2: Order Files by Dependency

Sort files in this order:
1. Config files (package.json, Cargo.toml, pyproject.toml, etc.)
2. Utility files (utils/, helpers/, lib/)
3. Core files (models/, core/, schema/)
4. Feature files (services/, handlers/, controllers/)
5. Test files (test/, spec/, *_test.*)
6. Documentation (.md files, docs/)

### Step 3: Create One Commit Per File

For each file in order:

```bash
# Reset staging
git reset HEAD

# Stage single file
git add <file>

# Read file to understand it
cat <file>

# Commit (with optional timestamp)
git commit -m "descriptive message"

# Or with timestamp
GIT_AUTHOR_DATE="2025-01-03 14:32:17 +0000" GIT_COMMITTER_DATE="2025-01-03 14:32:17 +0000" git commit -m "message"
```

## Example

User: "Create atomic commits for my changes"

Files detected: package.json, src/utils/auth.js, src/services/user.js, tests/user.test.js, README.md

Commits created:
1. `chore: add project dependencies` (package.json)
2. `feat(utils): add authentication helpers` (src/utils/auth.js)
3. `feat(services): add user service` (src/services/user.js)
4. `test: add user service tests` (tests/user.test.js)
5. `docs: add project readme` (README.md)

## With Time Spread

User: "Create atomic commits spread over 2 hours starting at 9am"

Calculate varied timestamps:
- 09:12:34, 09:38:17, 10:04:52, 10:31:08, 10:55:43

## When to Use

- Quick splitting without complex analysis
- When each file represents a distinct change
- Simple projects with clear file boundaries
