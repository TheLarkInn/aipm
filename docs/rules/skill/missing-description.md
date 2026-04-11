# skill/missing-description

**Severity:** warning
**Fixable:** No

Checks that every SKILL.md file includes a `description` field in the YAML frontmatter. A description helps users understand the purpose of the skill when browsing or listing installed plugins.

## Examples

### Incorrect
```markdown
---
name: my-skill
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
Add a `description` field to the YAML frontmatter. Write a short sentence summarising what the skill does.

## See also

- [skill/missing-name](missing-name.md) — validates the skill's `name` field
- [skill/description-too-long](description-too-long.md) — validates the description length limit
- [Creating a plugin](../../guides/creating-a-plugin.md) — how to scaffold a new plugin with all required frontmatter
- [Using `aipm lint`](../../guides/lint.md) — CLI reference for running the lint system
