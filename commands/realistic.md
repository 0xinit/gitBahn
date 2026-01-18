---
description: Create realistic commits that simulate human development flow
---

# Realistic Commits

Create commits that look like natural human development - split by language constructs and ordered by dependency.

## Quick Start

1. Stage all your changes: `stage_all`
2. Get split suggestion: `suggest_realistic_split`
3. Follow the suggested groups to create commits

## How It Works

The `suggest_realistic_split` tool analyzes your staged files and:

1. **Parses by language** - Splits files into logical chunks:
   - Python: imports → classes → functions
   - Rust: use/mod → structs/enums → impl/functions
   - JS/TS: imports → components/functions
   - Go: package/imports → types → functions
   - Ruby: requires → classes/modules → methods

2. **Orders by dependency**:
   - Config files first (Cargo.toml, package.json)
   - Utilities/helpers
   - Core/models
   - Features/services
   - Tests
   - Documentation last

3. **Returns grouped suggestions** with files and hints

## Workflow

```
1. stage_all                           # Stage everything
2. suggest_realistic_split             # Get groupings
3. For each group:
   a. unstage_all                      # Reset staging
   b. stage_files [group files]        # Stage this group
   c. get_diff                         # See what's staged
   d. [Generate commit message]        # You analyze the diff
   e. create_commit {message, timestamp}  # Commit with optional timestamp
```

## Example

```
User: "Create realistic commits spread over 4 hours starting at 10am"

Claude Code:
1. Calls stage_all
2. Calls suggest_realistic_split → gets 8 groups
3. Plans timestamps: 10:14, 10:47, 11:23, 11:58, 12:34, 13:12, 13:41, 14:08
4. For each group:
   - unstage_all
   - stage_files with group's files
   - get_diff to see changes
   - Generates message like "feat(core): add user model"
   - create_commit with message and timestamp
```

## Options

- `target_commits`: Merge groups to hit a specific number (e.g., 5 commits)

## Tips

- Vary timestamps realistically (not exact intervals)
- Use realistic seconds (14:32:47 not 14:30:00)
- Mix commit sizes (some small, some larger)
