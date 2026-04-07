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

### Claude Code (PascalCase)

| Event | Description |
|---|---|
| `PreToolUse` | Fires before a tool is invoked |
| `PostToolUse` | Fires after a tool completes successfully |
| `PostToolUseFailure` | Fires after a tool invocation fails |
| `Notification` | General notification event |
| `SessionStart` | Fires when a session starts |
| `Stop` | Fires when the agent stops normally |
| `StopFailure` | Fires when the agent stops due to an error |
| `SubagentStart` | Fires when a subagent starts |
| `SubagentStop` | Fires when a subagent stops |
| `PreCompact` | Fires before context compaction |
| `PostCompact` | Fires after context compaction |
| `SessionEnd` | Fires when a session ends |
| `PermissionRequest` | Fires when a permission is requested |
| `Setup` | Fires during initial setup |
| `TeammateIdle` | Fires when a teammate becomes idle |
| `TaskCreated` | Fires when a task is created |
| `TaskCompleted` | Fires when a task completes |
| `UserPromptSubmit` | Fires when the user submits a prompt |
| `ToolError` | Fires when a tool returns an error |
| `Elicitation` | Fires during elicitation |
| `ElicitationResult` | Fires when elicitation produces a result |
| `ConfigChange` | Fires when configuration changes |
| `InstructionsLoaded` | Fires when instructions are loaded |
| `WorktreeCreate` | Fires when a git worktree is created |
| `WorktreeRemove` | Fires when a git worktree is removed |
| `CwdChanged` | Fires when the working directory changes |
| `FileChanged` | Fires when a file changes |

### Copilot CLI (camelCase)

| Event | Description |
|---|---|
| `sessionStart` | Fires when a session starts |
| `sessionEnd` | Fires when a session ends |
| `userPromptSubmitted` | Fires when the user submits a prompt |
| `preToolUse` | Fires before a tool is invoked |
| `postToolUse` | Fires after a tool completes |
| `errorOccurred` | Fires when an error occurs |
| `agentStop` | Fires when the agent stops |
| `subagentStop` | Fires when a subagent stops |
| `subagentStart` | Fires when a subagent starts |
| `preCompact` | Fires before context compaction |

> **Note:** Hooks in `.claude/` and `.github/` are validated against their respective tool's event set. Hooks in `.ai/` marketplace plugins are validated against the union of all supported events.

See also: [`hook/legacy-event-name`](legacy-event-name.md) for deprecated Copilot event names.
