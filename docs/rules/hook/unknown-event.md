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

| Event | Description |
|-------|-------------|
| `PreToolUse` | Before any tool is invoked |
| `PostToolUse` | After a tool completes successfully |
| `PostToolUseFailure` | After a tool invocation fails |
| `Notification` | When a notification is sent |
| `SessionStart` | When a session starts |
| `Stop` | When the agent stops normally |
| `StopFailure` | When the agent stops due to failure |
| `SubagentStart` | When a subagent starts |
| `SubagentStop` | When a subagent stops |
| `PreCompact` | Before context compaction |
| `PostCompact` | After context compaction |
| `SessionEnd` | When a session ends |
| `PermissionRequest` | When a permission request is made |
| `Setup` | During initial setup |
| `TeammateIdle` | When a teammate becomes idle |
| `TaskCreated` | When a task is created |
| `TaskCompleted` | When a task completes |
| `UserPromptSubmit` | When the user submits a prompt |
| `ToolError` | When a tool returns an error |
| `Elicitation` | When elicitation begins |
| `ElicitationResult` | When an elicitation result is received |
| `ConfigChange` | When configuration changes |
| `InstructionsLoaded` | When instructions are loaded |
| `WorktreeCreate` | When a worktree is created |
| `WorktreeRemove` | When a worktree is removed |
| `CwdChanged` | When the working directory changes |
| `FileChanged` | When a file changes |

### Copilot CLI (10 events, `camelCase`)

| Event | Description |
|-------|-------------|
| `sessionStart` | When a session starts |
| `sessionEnd` | When a session ends |
| `userPromptSubmitted` | When the user submits a prompt |
| `preToolUse` | Before any tool is invoked |
| `postToolUse` | After a tool completes |
| `errorOccurred` | When an error occurs |
| `agentStop` | When the agent stops |
| `subagentStop` | When a subagent stops |
| `subagentStart` | When a subagent starts |
| `preCompact` | Before context compaction |

See also: [`hook/legacy-event-name`](./legacy-event-name.md) for legacy Copilot `PascalCase` names that are automatically mapped to their `camelCase` equivalents.
