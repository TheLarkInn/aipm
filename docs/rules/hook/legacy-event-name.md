# hook/legacy-event-name

**Severity:** warning
**Fixable:** No

Checks that Copilot CLI hook event names in `hooks.json` use the canonical `camelCase` naming convention rather than a legacy `PascalCase` alias. Copilot CLI normalizes these legacy names internally, but using them is discouraged and they may stop being recognized in a future release.

This rule applies only to Copilot CLI hooks (`.github/` source). Claude Code uses `PascalCase` event names natively and is unaffected.

## Examples

### Incorrect

```json
{
  "Stop": [{ "hooks": [{ "type": "command", "command": "./cleanup.sh" }] }]
}
```

*(`Stop` is the legacy name; Copilot CLI maps it to `agentStop`)*

### Correct

```json
{
  "agentStop": [{ "hooks": [{ "type": "command", "command": "./cleanup.sh" }] }]
}
```

## How to fix

Rename the event key to its canonical `camelCase` equivalent. Run `aipm migrate` to have the migration tool update hook event names automatically.

## Legacy-to-canonical mapping

| Legacy (`PascalCase`) | Canonical (`camelCase`) |
|-----------------------|------------------------|
| `SessionStart` | `sessionStart` |
| `SessionEnd` | `sessionEnd` |
| `UserPromptSubmit` | `userPromptSubmitted` |
| `PreToolUse` | `preToolUse` |
| `PostToolUse` | `postToolUse` |
| `PostToolUseFailure` | `errorOccurred` |
| `ErrorOccurred` | `errorOccurred` |
| `Stop` | `agentStop` |
| `SubagentStop` | `subagentStop` |
| `PreCompact` | `preCompact` |
