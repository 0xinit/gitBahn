---
description: Create atomic commits - one file per commit
---

# Atomic Commits

Split changes so each file gets its own commit. Simple and quick.

## Quick Start

1. Stage all changes: `stage_all`
2. Get split suggestion: `suggest_atomic_split`
3. Create commits for each file

## How It Works

The `suggest_atomic_split` tool:
- Lists each staged file as its own group
- Orders by dependency (config → utils → core → features → tests → docs)
- Provides hints about file type and size

## Workflow

```
1. stage_all
2. suggest_atomic_split
3. For each file:
   a. unstage_all
   b. stage_files [single file]
   c. get_diff
   d. [Generate commit message]
   e. create_commit {message}
```

## Example Output

```
# ATOMIC Split Suggestion

**5 commit groups** suggested

### Group 1 - Add config: package.json
- Files: package.json
- Hint: json (25 lines)

### Group 2 - Add utils.ts
- Files: src/utils.ts
- Hint: typescript (80 lines)

### Group 3 - Add api.ts
- Files: src/api.ts
- Hint: typescript (120 lines)

### Group 4 - Add tests: api.test.ts
- Files: src/api.test.ts
- Hint: typescript (60 lines)

### Group 5 - Add docs: README.md
- Files: README.md
- Hint: markdown (45 lines)
```

## When to Use

- Quick splitting without complex analysis
- When each file represents a distinct change
- Simple projects with clear file boundaries
