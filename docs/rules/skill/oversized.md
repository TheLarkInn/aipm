# skill/oversized

**Severity:** warning
**Fixable:** No

Checks that the total character count of the SKILL.md file does not exceed 15 000 characters. Oversized skill files slow down context loading and may exceed limits imposed by AI runtimes.

## Why 15 000 characters?

The 15 000 character budget is derived from **Copilot CLI's `SKILL_CHAR_BUDGET`** default. Copilot imposes this limit when loading skill context into the model prompt; exceeding it causes the skill to be silently truncated or rejected.

Claude Code does not currently enforce a hard character limit on skill files, but AIPM uses the Copilot limit as a **tool-agnostic quality guardrail**: a skill that fits within 15 000 characters is almost certainly well-scoped and will work reliably across all supported AI runtimes. Keeping skills concise also reduces token usage and improves agent response quality.

## Examples

### Incorrect
```markdown
---
name: my-skill
description: Does something useful
---
[...content exceeding 15 000 characters...]
```

### Correct
```markdown
---
name: my-skill
description: Does something useful
---
Concise skill instructions that stay under the 15 000 character limit.
```

## How to fix
Reduce the file size below 15 000 characters. Consider splitting large skills into multiple smaller, focused skill files, or moving lengthy reference material to an external resource linked from the skill body.

## See also

- [skill/missing-name](missing-name.md) — validates that a `name` field is present
- [skill/missing-description](missing-description.md) — validates that a `description` field is present
- [Creating a plugin](../../guides/creating-a-plugin.md) — how to scaffold a new plugin
- [Using `aipm lint`](../../guides/lint.md) — CLI reference for running the lint system
