---
date: 2026-03-24
researcher: Claude Opus 4.6
branch: main
repository: aipm
topic: "Claude Code Hooks, Settings, and Output Styles Format"
tags: [research, claude-code, hooks, settings, output-styles, configuration]
status: complete
last_updated: 2026-03-24
last_updated_by: Claude Opus 4.6
---

# Claude Code Hooks, Settings, and Output Styles

## Summary

Hooks live inside `settings.json` under the `"hooks"` key at the project level (NOT in a separate `hooks/hooks.json` — that format only exists inside plugins). Settings use JSON with ~40+ configurable keys. Output styles are Markdown files with YAML frontmatter stored in `.claude/output-styles/`.

## 1. Hooks

### Location

Project-level hooks are in `.claude/settings.json` under `"hooks"`:
```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [{ "type": "command", "command": "./scripts/validate.sh" }]
      }
    ]
  }
}
```

Plugin hooks use a standalone file: `hooks/hooks.json` with `{ "hooks": { ... } }` wrapper.

### Event Types (22 total)

`SessionStart`, `SessionEnd`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `PermissionRequest`, `Notification`, `SubagentStart`, `SubagentStop`, `Stop`, `StopFailure`, `TeammateIdle`, `TaskCompleted`, `InstructionsLoaded`, `ConfigChange`, `WorktreeCreate`, `WorktreeRemove`, `PreCompact`, `PostCompact`, `Elicitation`, `ElicitationResult`.

### Handler Types

| Type | Key Field | Description |
|:-----|:----------|:------------|
| `command` | `command` | Shell command to execute |
| `http` | `url` | POST request to URL |
| `prompt` | `prompt` | LLM evaluation with `$ARGUMENTS` |
| `agent` | (config) | Agentic verifier with tools |

### Handler Fields

| Field | Type | Description |
|:------|:-----|:------------|
| `type` | string | `command`, `http`, `prompt`, `agent` |
| `command` | string | Shell command (command type) |
| `url` | string | Endpoint (http type) |
| `prompt` | string | LLM prompt (prompt type) |
| `timeout` | number | Timeout in ms |
| `async` | bool | Run asynchronously |
| `statusMessage` | string | UI message during execution |
| `once` | bool | Run only once per session |

## 2. Settings

### Location

| Scope | File | Shared? |
|:------|:-----|:--------|
| Project | `.claude/settings.json` | Yes (git) |
| Local | `.claude/settings.local.json` | No (gitignored) |
| User | `~/.claude/settings.json` | No |

### Key Fields

`permissions` (allow/ask/deny arrays), `env`, `hooks`, `model`, `outputStyle`, `agent`, `sandbox`, `attribution`, `extraKnownMarketplaces`, plus many more.

### Plugin Settings Limitation

Plugin `settings.json` currently **only supports the `agent` field**. Permissions, env, model, and other settings cannot be distributed via plugins.

## 3. Output Styles

### Location

- User: `~/.claude/output-styles/`
- Project: `.claude/output-styles/`

### Format

Markdown with YAML frontmatter:

```markdown
---
name: concise
description: Minimal output
keep-coding-instructions: true
---

Be extremely concise. No preamble, no explanations.
```

### Frontmatter Fields

| Field | Required | Type | Description |
|:------|:---------|:-----|:------------|
| `name` | Yes | string | Display name |
| `description` | No | string | What the style does |
| `keep-coding-instructions` | No | bool | Keep default coding instructions (default: false) |

### Activation

Set `"outputStyle": "StyleName"` in settings, or use `/config` menu.

## Sources

- [Hooks reference](https://code.claude.com/docs/en/hooks)
- [Settings](https://code.claude.com/docs/en/settings)
- [Output styles](https://code.claude.com/docs/en/output-styles)
- [Plugins reference](https://code.claude.com/docs/en/plugins-reference)
