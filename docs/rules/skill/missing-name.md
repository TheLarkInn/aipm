# skill/missing-name

**Severity:** warning
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

## See also

- [skill/missing-description](missing-description.md) — validates the skill's `description` field
- [skill/name-invalid-chars](name-invalid-chars.md) — validates that the name uses allowed characters
- [skill/name-too-long](name-too-long.md) — validates the name length limit
- [Creating a plugin](../../guides/creating-a-plugin.md) — how to scaffold a new plugin with all required frontmatter
- [Using `aipm lint`](../../guides/lint.md) — CLI reference for running the lint system
