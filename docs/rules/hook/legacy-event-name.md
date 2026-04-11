# hook/legacy-event-name

**Severity:** warning
**Fixable:** No

Checks that Copilot CLI hook files (`hooks.json`) do not use legacy `PascalCase` event names
that Copilot internally normalizes to `camelCase`. These legacy names still work today, but
relying on the internal normalization is brittle — use the canonical `camelCase` names directly.

This rule only fires for hooks inside `.github/` or `.ai/` marketplace directories. Claude Code
hooks (`.claude/`) use `PascalCase` natively and are **not** affected.

## Legacy-to-canonical mapping

| Legacy (`PascalCase`) | Canonical (`camelCase`) |
|-----------------------|-------------------------|
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

## Examples

### Incorrect

```json
{
  "Stop": [],
  "UserPromptSubmit": []
}
```

### Correct

```json
{
  "agentStop": [],
  "userPromptSubmitted": []
}
```

## How to fix

Rename each legacy `PascalCase` event name to its canonical `camelCase` equivalent using the
mapping table above (e.g. `Stop` → `agentStop`, `UserPromptSubmit` → `userPromptSubmitted`).

Run `aipm migrate` on an existing `.github/` directory to have the migration tool perform these
renames automatically.

## See also

- [hook/unknown-event](unknown-event.md) — flags completely unrecognised event names
- [Migrating existing configurations](../../guides/migrate.md) — `aipm migrate` can rename legacy event names automatically
- [Using `aipm lint`](../../guides/lint.md) — CLI reference for running the lint system
