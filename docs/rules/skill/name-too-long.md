# skill/name-too-long

**Severity:** warning
**Fixable:** No

Checks that the `name` field in SKILL.md frontmatter is no longer than 64 characters. This limit is derived from the Copilot CLI Zod schema (`z.string().max(64)`). Long names are harder to read in CLI output and may be truncated in some UI contexts.

## Examples

### Incorrect
```markdown
---
name: this-skill-name-is-far-too-long-and-exceeds-the-sixty-four-char-limit
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
Shorten the name to 64 characters or fewer. Use a concise, descriptive identifier that clearly conveys the skill's purpose.

## See also

- [skill/missing-name](missing-name.md) — validates that a `name` field is present
- [skill/name-invalid-chars](name-invalid-chars.md) — validates that the name uses allowed characters
- [skill/description-too-long](description-too-long.md) — validates the description length limit
- [Creating a plugin](../../guides/creating-a-plugin.md) — how to scaffold a new plugin with correct naming
- [Using `aipm lint`](../../guides/lint.md) — CLI reference for running the lint system
