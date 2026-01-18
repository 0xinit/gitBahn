---
description: Create realistic commits that simulate human development flow
---

# Realistic Commits

Create commits that look like natural human development - multiple small commits spread over time.

## Instructions

When asked to create realistic commits:

1. **Get the full diff** using `get_diff` tool
2. **Analyze and split** the changes into logical chunks:
   - Group by feature/purpose (not just by file)
   - Order like a developer would: config -> utils -> core -> features -> tests -> docs
   - Aim for 5-15 commits depending on change size
3. **Plan timestamps** spread over the requested duration:
   - Vary gaps between commits (30min to 3hrs)
   - Use realistic seconds (not :00 or :30)
   - Consider working hours if appropriate
4. **For each logical chunk**:
   - Stage the relevant files using `stage_files`
   - Generate an appropriate commit message
   - Call `create_commit` with message and timestamp

## Example

User: "Create realistic commits for these changes spread over 6 hours starting at 9am today"

1. Get diff, identify logical groups:
   - Config changes (package.json, tsconfig)
   - New utility functions
   - Core feature implementation
   - Tests
   - Documentation

2. Create commits with timestamps:
   - 09:14:32 - "chore: update dependencies"
   - 10:47:18 - "feat(utils): add string helpers"
   - 12:23:51 - "feat(auth): implement login flow"
   - 14:08:27 - "test(auth): add login tests"
   - 15:31:44 - "docs: update API documentation"

## Timestamp Format

Use format: `YYYY-MM-DD HH:MM:SS` (e.g., "2025-01-15 14:32:47")

The `create_commit` tool accepts a `timestamp` parameter for backdating.

## Tips for Realistic History

- Vary commit sizes (some small, some medium)
- Include occasional typo fixes or minor adjustments
- Group related changes but not too perfectly
- Space commits unevenly (humans don't commit every 30 mins exactly)
