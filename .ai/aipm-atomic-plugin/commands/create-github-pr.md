---
description: Commit unstaged changes, push changes, submit a pull request to GitHub.
model: opus
allowed-tools: Bash(git:*), Bash(gh:*), Glob, Grep, NotebookRead, Read, SlashCommand
argument-hint: [code-path]
---

# Create Pull Request Command

Commit changes using the `/commit` command, push all changes, and submit a pull request to GitHub.

## Behavior
- Creates logical commits for unstaged changes
- Pushes branch to remote
- Creates pull request on GitHub with proper title and description

## Tools
- Use the GitHub CLI (`gh`) to create pull requests
- Use `gh pr create --title "..." --body "..."` for PR creation
- Use `gh pr view` to verify the PR was created

## PR Format

**Instructions:**
- Generate a clear PR title summarizing the changes
- Write a description covering:
  - What changed and why
  - Testing performed
  - Any follow-up items
