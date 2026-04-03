# skill/oversized

**Severity:** warning
**Fixable:** No

Checks that the total character count of the SKILL.md file does not exceed 15 000 characters. Oversized skill files slow down context loading and may exceed limits imposed by AI runtimes.

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
