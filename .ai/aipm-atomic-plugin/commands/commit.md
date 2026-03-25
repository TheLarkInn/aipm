---
description: Create well-formatted commits with clear, descriptive messages.
model: opus
allowed-tools: Bash(git add:*), Bash(git status:*), Bash(git commit:*), Bash(git diff:*), Bash(git log:*)
argument-hint: [message] | --amend
---

# Smart Git Commit

Create well-formatted commit: $ARGUMENTS

## Current Repository State

- Git status: !`git status --porcelain`
- Current branch: !`git branch --show-current`
- Staged changes: !`git diff --cached --stat`
- Unstaged changes: !`git diff --stat`
- Recent commits: !`git log --oneline -5`

## What This Command Does

1. Checks which files are staged with `git status`
2. If 0 files are staged, automatically adds all modified and new files with `git add`
3. Performs a `git diff` to understand what changes are being committed
4. Analyzes the diff to determine if multiple distinct logical changes are present
5. If multiple distinct changes are detected, suggests breaking the commit into multiple smaller commits
6. Creates a clear, descriptive commit message

## Best Practices for Commits

### Atomic Commits

- Each commit should represent a single logical change
- If you're changing multiple unrelated things, split them into separate commits
- A commit should be able to be reverted without affecting unrelated functionality

### Good Commit Messages

A good commit message should:

1. **Have a clear, concise subject line** (50 characters or less preferred)
   - Use imperative mood ("Add feature" not "Added feature")
   - Capitalize the first letter
   - Don't end with a period

2. **Include a body when needed** (wrapped at 72 characters)
   - Explain the "what" and "why", not the "how"
   - Separate from subject with a blank line
   - Use bullet points for multiple items

### Commit Message Examples

#### Simple change
```
Fix null pointer exception in user authentication
```

#### Change with body
```
Add caching layer for API responses

Implement an in-memory cache with 5-minute TTL to reduce
load on the backend services. This addresses the performance
issues reported in production during peak hours.

- Add CacheManager class with configurable TTL
- Integrate cache with ApiClient
- Add cache invalidation on user logout
```

#### Bug fix
```
Fix race condition in file upload handler

The previous implementation could fail when multiple files
were uploaded simultaneously due to shared state. Now each
upload gets its own context.
```

### What NOT to Do

- Don't make vague commits like "fix stuff" or "updates"
- Don't combine unrelated changes in one commit
- Don't include generated files unless necessary
- Don't commit broken code to shared branches

## Attributing AI-Assisted Code Authorship

When using AI tools to generate code, maintain transparency about authorship by using Git trailers:

```
Assistant-model: Claude Code
```

Trailers can be added with the `--trailer` option:

```bash
git commit --message "Implement feature" --trailer "Assistant-model: Claude Code"
```

View commits with assistant attribution:

```bash
git log --color --pretty=format:"%C(yellow)%h%C(reset) %C(blue)%an%C(reset) [%C(magenta)%(trailers:key=Assistant-model,valueonly=true,separator=%x2C)%C(reset)] %s%C(bold cyan)%d%C(reset)"
```

## Important Notes

- By default, pre-commit checks (defined in `.pre-commit-config.yaml`) will run to ensure code quality
  - IMPORTANT: DO NOT SKIP pre-commit checks
- ALWAYS attribute AI-Assisted Code Authorship
- If specific files are already staged, the command will only commit those files
- If no files are staged, it will automatically stage all modified and new files
- The commit message will be constructed based on the changes detected
- Before committing, review the diff to identify if multiple commits would be more appropriate
- If suggesting multiple commits, help stage and commit the changes separately
- Always review the commit diff to ensure the message matches the changes


