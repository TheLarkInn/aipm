---
date: 2026-03-24
researcher: Claude Opus 4.6
branch: main
repository: aipm
topic: "Claude Code Agent/Subagent Definition Format"
tags: [research, claude-code, agents, subagents, yaml, frontmatter]
status: complete
last_updated: 2026-03-24
last_updated_by: Claude Opus 4.6
---

# Claude Code Agent/Subagent Definition Format

## Summary

Claude Code subagents are defined as Markdown files with YAML frontmatter stored in `.claude/agents/` (project) or `~/.claude/agents/` (user). The frontmatter defines metadata and configuration; the markdown body becomes the system prompt. Plugin-shipped agents live in the plugin's `agents/` directory but cannot use `hooks`, `mcpServers`, or `permissionMode` fields.

## File Locations

| Scope | Location | Priority |
|:------|:---------|:---------|
| CLI flag | `--agents` JSON | 1 (highest) |
| Project | `.claude/agents/` | 2 |
| User | `~/.claude/agents/` | 3 |
| Plugin | `<plugin>/agents/` | 4 (lowest) |

When multiple agents share the same name, higher-priority location wins.

## File Format

```markdown
---
name: security-reviewer
description: Reviews code for security vulnerabilities
tools: Read, Grep, Glob, Bash
model: sonnet
maxTurns: 20
---

You are a security code reviewer. Analyze code for OWASP top 10...
```

## YAML Frontmatter Fields

| Field | Required | Type | Description |
|:------|:---------|:-----|:------------|
| `name` | Yes | string | Unique identifier (lowercase + hyphens) |
| `description` | Yes | string | When Claude should delegate to this agent |
| `tools` | No | string | Comma-separated tool list (inherits all if omitted) |
| `disallowedTools` | No | string | Tools to deny |
| `model` | No | string | `sonnet`, `opus`, `haiku`, full model ID, or `inherit` |
| `permissionMode` | No | string | `default`, `acceptEdits`, `dontAsk`, `bypassPermissions`, `plan` |
| `maxTurns` | No | number | Max agentic turns |
| `skills` | No | string | Skills to preload into context |
| `mcpServers` | No | list | MCP servers (inline or reference) |
| `hooks` | No | object | Lifecycle hooks scoped to agent |
| `memory` | No | string | `user`, `project`, `local` |
| `background` | No | bool | Always run in background |
| `effort` | No | string | `low`, `medium`, `high`, `max` |
| `isolation` | No | string | `worktree` for isolated git worktree |

## Plugin Agent Restrictions

Plugin-shipped agents strip these fields for security: `hooks`, `mcpServers`, `permissionMode`.

## Source

- [Create custom subagents](https://code.claude.com/docs/en/sub-agents)
