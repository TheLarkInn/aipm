# skill/missing-name

**Severity:** error
**Fixable:** No

Checks that every SKILL.md file includes a `name` field in the YAML frontmatter. The name is required to identify and reference the skill within the plugin manifest.

## Examples

### Incorrect
```markdown
---
description: Does something useful
shell: bash
---
Skill instructions here...
```

### Correct
```markdown
---
name: my-skill
description: Does something useful
shell: bash
---
Skill instructions here...
```

## How to fix
Add a `name` field to the YAML frontmatter at the top of your SKILL.md file. The value should be a short, lowercase identifier for the skill.
