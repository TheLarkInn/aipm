# skill/name-too-long

**Severity:** warning
**Fixable:** No

Checks that the `name` field in SKILL.md frontmatter is no longer than 60 characters. Long names are harder to read in CLI output and may be truncated in some UI contexts.

## Examples

### Incorrect
```markdown
---
name: this-skill-name-is-far-too-long-and-exceeds-the-sixty-character-limit
description: Does something useful
---
```

### Correct
```markdown
---
name: my-skill
description: Does something useful
---
```

## How to fix
Shorten the name to 60 characters or fewer. Use a concise, descriptive identifier that clearly conveys the skill's purpose.
