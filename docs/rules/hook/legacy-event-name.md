# hook/legacy-event-name

**Severity:** warning
**Fixable:** No

Checks that hook event names in `hooks.json` use the current PascalCase naming convention rather than a legacy snake_case or camelCase alias. Legacy names are deprecated and may be removed in a future release.

## Examples

### Incorrect
```json
{
  "hooks": [
    { "event": "pre_install", "command": "./prepare.sh" }
  ]
}
```

### Correct
```json
{
  "hooks": [
    { "event": "PreInstall", "command": "./prepare.sh" }
  ]
}
```

## How to fix
Rename the event value to its canonical camelCase equivalent. Run `aipm migrate` to have the migration tool update hook event names automatically.

## Legacy → canonical name mapping (Copilot CLI)

| Legacy name (PascalCase) | Canonical name (camelCase) |
|---|---|
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

> **Claude Code** uses PascalCase event names natively and does not have legacy aliases. This rule applies only to Copilot CLI hooks (`.github/` source or `.ai/` marketplace hooks that use legacy names).

See also: [`hook/unknown-event`](unknown-event.md) for the full list of supported event names.
