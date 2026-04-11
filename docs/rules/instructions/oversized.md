# instructions/oversized

**Severity:** warning
**Fixable:** No

Checks that instruction files (e.g. `CLAUDE.md`, `COPILOT.md`) do not exceed configured line and character limits. Oversized instruction files increase token consumption and may be silently truncated by AI runtimes.

## Default limits

| Limit | Default | Config key |
|-------|---------|------------|
| Lines | 100 | `lines` |
| Characters | 15 000 | `characters` |

Both thresholds are checked independently — a file that exceeds either limit (or both) receives a separate diagnostic per violation.

## Why these limits?

**100 lines** is a pragmatic threshold that encourages concise, focused instruction files. Instructions that run longer than a hundred lines often contain duplicated guidance, verbose prose, or content that belongs in linked skill files rather than the top-level instruction file.

**15 000 characters** mirrors the `SKILL_CHAR_BUDGET` default used by Copilot CLI, making the rule consistent with the existing `skill/oversized` guardrail and ensuring instruction files remain safe across all supported AI runtimes.

## Import resolution

When `resolve-imports = true`, the rule follows `@path/to/file.md` imports and relative Markdown links transitively before checking the totals. The diagnostic will mention the resolved total alongside the direct file count:

```
instruction file exceeds 100 line limit (resolved total: 152 lines, direct: 48 lines)
```

Import resolution is **disabled by default** to keep checks fast. Enable it when your instruction files use `@path/to/file.md` or `[…](link.md)` patterns to compose content from multiple files.

## Examples

### Incorrect — exceeds line limit

```markdown
# My project instructions

[...more than 100 lines of guidance...]
```

### Incorrect — exceeds character limit

```markdown
# My project instructions

[...content exceeding 15 000 characters...]
```

### Correct

```markdown
# My project instructions

Keep instruction files concise and focused on the most important context.
For detailed guidance, link to skill files or external resources.
```

## How to fix

Reduce the file size below both thresholds. Common strategies:

- Move detailed or reusable content into separate `.ai/<plugin>/skills/` skill files
- Split a monolithic instruction file into multiple focused files imported via `@path/to/file.md`
- Remove redundant or verbose prose — prefer imperative bullets over narrative paragraphs
- Link to external resources instead of inlining them

## Tuning thresholds

Override the defaults in `aipm.toml` using the inline table syntax:

```toml
[workspace.lints]
# Raise the line budget and enable import resolution
"instructions/oversized" = { level = "warn", lines = 200, characters = 30000, resolve-imports = true }
```

To suppress the rule entirely:

```toml
[workspace.lints]
"instructions/oversized" = "allow"
```

Available options:

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `level` | string | `"warn"` | Severity: `"error"`, `"warn"`, or `"allow"` |
| `lines` | integer | `100` | Maximum line count |
| `characters` | integer | `15000` | Maximum character count |
| `resolve-imports` | boolean | `false` | Follow `@path/to/file.md` imports and relative links before checking limits |

## See also

- [skill/oversized](../skill/oversized.md) — analogous check for SKILL.md files
- [Configuring lint](../../guides/configuring-lint.md) — how to tune severity and thresholds
- [Using `aipm lint`](../../guides/lint.md) — CLI reference for running the lint system
