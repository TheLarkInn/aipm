# hook/unknown-event

**Severity:** error
**Fixable:** No

Checks that every event name declared in a `hooks.json` file is a recognised aipm hook event. Unknown event names are silently ignored at runtime, meaning the hook will never fire.

## Examples

### Incorrect
```json
{
  "hooks": [
    { "event": "on_install", "command": "./setup.sh" }
  ]
}
```

### Correct
```json
{
  "hooks": [
    { "event": "PostInstall", "command": "./setup.sh" }
  ]
}
```

## How to fix
Replace the unknown event name with a valid hook event name from the tables below.

## Supported events

### Claude Code (27 events, `PascalCase`)

| Event | Event | Event |
|---|---|---|
| `PreToolUse` | `PostToolUse` | `PostToolUseFailure` |
| `Notification` | `SessionStart` | `SessionEnd` |
| `Stop` | `StopFailure` | `SubagentStart` |
| `SubagentStop` | `PreCompact` | `PostCompact` |
| `PermissionRequest` | `Setup` | `TeammateIdle` |
| `TaskCreated` | `TaskCompleted` | `UserPromptSubmit` |
| `ToolError` | `Elicitation` | `ElicitationResult` |
| `ConfigChange` | `InstructionsLoaded` | `WorktreeCreate` |
| `WorktreeRemove` | `CwdChanged` | `FileChanged` |

### Copilot CLI (10 events, `camelCase`)

| Event |
|---|
| `sessionStart` |
| `sessionEnd` |
| `userPromptSubmitted` |
| `preToolUse` |
| `postToolUse` |
| `errorOccurred` |
| `agentStop` |
| `subagentStop` |
| `subagentStart` |
| `preCompact` |

See also: [`hook/legacy-event-name`](legacy-event-name.md) for the Copilot CLI `PascalCase` → `camelCase` migration table.
