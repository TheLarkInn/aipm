# skill/name-invalid-chars

**Severity:** warning
**Fixable:** No

Checks that the `name` field in SKILL.md frontmatter contains only alphanumeric characters, hyphens (`-`), and underscores (`_`). Names with spaces or special characters can break plugin resolution and shell integration.

## Examples

### Incorrect
```markdown
---
name: my skill!
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
Use only alphanumeric characters, hyphens, and underscores in the name. Replace spaces with hyphens and remove any other special characters.
