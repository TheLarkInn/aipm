# hook/unknown-event

**Severity:** error
**Fixable:** No

Checks that every event name declared in a `hooks.json` file is a recognised hook event for the target AI tool. Unknown event names are silently ignored at runtime, meaning the hook will never fire.

Event names are **case-sensitive** and depend on the tool:

- **Claude Code** uses `PascalCase` (e.g., `PreToolUse`, `SessionStart`)
- **Copilot CLI** uses `camelCase` (e.g., `preToolUse`, `sessionStart`)

## Examples

### Incorrect

```json
{
  "PreInstall": [{ "hooks": [{ "type": "command", "command": "./setup.sh" }] }]
}
```

*(`PreInstall` is not a valid event name for any supported tool)*

### Correct (Claude Code)

```json
{
  "PreToolUse": [{ "hooks": [{ "type": "command", "command": "./setup.sh" }] }]
}
```

### Correct (Copilot CLI)

```json
{
  "preToolUse": [{ "hooks": [{ "type": "command", "command": "./setup.sh" }] }]
}
```

## How to fix

Replace the unknown event name with a valid event name for your target tool. See the supported event lists below.

## Supported events

### Claude Code (27 events, `PascalCase`)

`PreToolUse` · `PostToolUse` · `PostToolUseFailure` · `Notification` · `SessionStart` ·
`Stop` · `StopFailure` · `SubagentStart` · `SubagentStop` · `PreCompact` · `PostCompact` ·
`SessionEnd` · `PermissionRequest` · `Setup` · `TeammateIdle` · `TaskCreated` ·
`TaskCompleted` · `UserPromptSubmit` · `ToolError` · `Elicitation` · `ElicitationResult` ·
`ConfigChange` · `InstructionsLoaded` · `WorktreeCreate` · `WorktreeRemove` ·
`CwdChanged` · `FileChanged`

### Copilot CLI (10 events, `camelCase`)

`sessionStart` · `sessionEnd` · `userPromptSubmitted` · `preToolUse` · `postToolUse` ·
`errorOccurred` · `agentStop` · `subagentStop` · `subagentStart` · `preCompact`
