# hook/unknown-event

**Severity:** error
**Fixable:** No

Checks that every event name declared in a `hooks.json` file is a recognised hook event for
at least one supported AI tool. Unknown event names are silently ignored at runtime, meaning
the hook will never fire.

## `hooks.json` format

Event names are **top-level object keys** (or keys inside a nested `"hooks"` object). The
structural keys `"version"`, `"disableAllHooks"`, and `"hooks"` are never treated as event names.

```json
{ "PostToolUse": [], "SessionStart": [] }
```

or with nesting:

```json
{ "hooks": { "PostToolUse": [], "SessionStart": [] } }
```

## Examples

### Incorrect

```json
{
  "on_install": []
}
```

### Correct

```json
{
  "PostToolUse": []
}
```

## Valid event names

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

> **Note:** Copilot also accepts the legacy `PascalCase` aliases listed in the
> [hook/legacy-event-name](./legacy-event-name.md) rule reference, but these will trigger
> a separate `hook/legacy-event-name` warning.

## How to fix

Replace the unknown event name with a valid hook event from the tables above.
For `.ai/` marketplace plugins (shared across tools) any event from either tool's list is
accepted.

## See also

- [hook/legacy-event-name](legacy-event-name.md) — warns when valid-but-deprecated PascalCase Copilot event names are used
- [Using `aipm lint`](../../guides/lint.md) — CLI reference for running the lint system
- [Configuring lint](../../guides/configuring-lint.md) — override rule severity or suppress rules per path
