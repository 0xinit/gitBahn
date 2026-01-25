---
description: Create granular commits - split by diff hunks
---

# Granular Commits

Split by diff hunks - allows splitting changes within a single file across multiple commits. Best for modified files with multiple logical changes.

## How to Use

When the user asks for "granular commits", follow this workflow:

### Step 1: Get the Diff with Hunks

```bash
git add -A
git diff --cached -U3
```

### Step 2: Parse Diff Hunks

Each hunk starts with `@@ -start,count +start,count @@ context`

Example diff:
```
+++ b/src/auth.rs
@@ -45,3 +45,15 @@ fn validate_user
  // changes here
@@ -120,2 +132,10 @@ fn hash_password
  // more changes
```

This shows 2 hunks in src/auth.rs at different locations.

### Step 3: Group Related Hunks

Group hunks that belong together:
- Hunks in the same function/method
- Hunks that are related conceptually
- Config changes separate from feature changes

### Step 4: Use Interactive Staging

For true hunk-level commits, use `git add -p`:

```bash
# Interactive staging - stage hunks one by one
git add -p <file>
```

This will show each hunk and ask:
- `y` - stage this hunk
- `n` - don't stage this hunk
- `s` - split into smaller hunks
- `q` - quit

### Step 5: Create Commits

After staging related hunks:

```bash
git commit -m "feat(auth): add user validation logic"
```

Repeat for remaining hunks.

## Example

User: "Create granular commits for my auth changes"

Diff analysis shows:
- src/auth.rs: 3 hunks (validate_user, hash_password, token refresh)
- src/config.rs: 1 hunk (auth config)

Commits created:
1. `feat(auth): add user validation function`
2. `feat(auth): implement password hashing`
3. `feat(auth): add token refresh logic`
4. `chore(config): add authentication settings`

## When to Use

- Modified files with multiple distinct changes
- Separating unrelated changes in the same file
- Creating clean history from large refactoring
