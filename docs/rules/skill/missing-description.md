# skill/missing-description

**Severity:** warning
**Fixable:** No

Checks that every SKILL.md file includes a non-empty `description` field in the YAML frontmatter. A description helps users understand the purpose of the skill when browsing or listing installed plugins, and it is what the engine uses to decide when to activate the skill.

A `description` that is present but empty or whitespace-only is reported as well: `SKILL.md field "description" is empty`.

## Examples

### Incorrect
```markdown
---
name: my-skill
shell: bash
---
Skill instructions here...
```

```markdown
---
name: my-skill
description: "   "
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
Add a `description` field to the YAML frontmatter with a short sentence summarising what the skill does. If the field is already present, give it a non-blank value.

## See also

- [skill/missing-name](missing-name.md) — validates the skill's `name` field
- [skill/description-too-long](description-too-long.md) — validates the description length limit
- [skill/description-invalid-chars](description-invalid-chars.md) — rejects `<` and `>` in Copilot-targeted descriptions
- [skill/invalid-frontmatter](invalid-frontmatter.md) — validates the frontmatter structure and field types
- [Creating a plugin](../../guides/creating-a-plugin.md) — how to scaffold a new plugin with all required frontmatter
- [Using `aipm lint`](../../guides/lint.md) — CLI reference for running the lint system
