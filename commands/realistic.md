---
description: Create realistic commits that simulate human development flow
---

# Realistic Commits

Create commits that look like natural human development - split by language constructs and ordered by dependency.

## How to Use

When the user asks for "realistic commits", follow this workflow:

### Step 1: Analyze Changes

```bash
git status --porcelain
git diff --cached --stat  # If files are staged
git diff --stat           # If files are unstaged
```

### Step 2: Stage Everything

```bash
git add -A
```

### Step 3: Analyze Files by Language

For each changed file, parse its content to identify:

**Python files (.py):**
- Imports (lines starting with `import` or `from`)
- Classes (lines starting with `class`)
- Functions (lines starting with `def` or `async def`)

**Rust files (.rs):**
- Use/mod statements
- Structs/enums
- Impl blocks and functions

**JavaScript/TypeScript (.js, .ts, .jsx, .tsx):**
- Import statements
- Components/classes
- Functions

**Go files (.go):**
- Package and imports
- Type definitions
- Functions

### Step 4: Order by Dependency

Commit in this order:
1. **Config files first** - Cargo.toml, package.json, pyproject.toml, etc.
2. **Utilities/helpers** - files with "util", "helper", "lib" in path
3. **Core/models** - files with "core", "model", "schema" in path
4. **Features/services** - main application code
5. **Tests** - files with "test" or "spec" in path
6. **Documentation** - .md files, docs/ folder

### Step 5: Create Commits with Timestamps

For each group:
1. Unstage everything: `git reset HEAD`
2. Stage the group's files: `git add <files>`
3. Generate a commit message based on what's staged
4. Create commit with timestamp:

```bash
GIT_AUTHOR_DATE="2025-01-03 11:17:32 +0000" GIT_COMMITTER_DATE="2025-01-03 11:17:32 +0000" git commit -m "message"
```

### Timestamp Guidelines

When user specifies a time spread (e.g., "spread over 4 hours starting at 10am"):
- Calculate realistic intervals (not exact, vary by 15-45 minutes)
- Use realistic seconds (14:32:47 not 14:30:00)
- Mix commit sizes (some quick commits, some longer gaps)

## Example

User: "Create realistic commits spread over 4 hours starting at 10am yesterday"

1. Stage all files
2. Identify groups: config → utils → models → services → tests → docs
3. Plan timestamps: 10:14, 10:47, 11:23, 11:58, 12:34, 13:12, 13:41
4. For each group:
   - Reset staging
   - Stage group files
   - Generate message like "feat(core): add user model"
   - Commit with calculated timestamp
