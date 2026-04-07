# skill/oversized

**Severity:** warning
**Fixable:** No

Checks that the total character count of the SKILL.md file does not exceed 15 000 characters.

## Why 15 000 characters?

This threshold is derived from the **Copilot CLI `SKILL_CHAR_BUDGET`** default of 15 000 characters — the point at which skill files start causing issues with Copilot's schema and API request limits.

Claude Code does not enforce a hard character limit on skill files. However, because AIPM is designed to be **tool-agnostic**, plugins should work consistently across all supported AI runtimes. Setting a single, conservative limit ensures skills load efficiently in every host and avoids subtle context-window degradation even on tools that don't enforce one explicitly.

If you need more content, consider linking to external documentation from the skill body rather than embedding it inline.

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
