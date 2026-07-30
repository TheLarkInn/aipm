# skill/name-not-kebab-case

**Severity:** warning
**Fixable:** No

Checks that the `name` field in SKILL.md frontmatter is lowercase kebab-case — groups of lowercase ASCII letters and digits joined by single hyphens:

```
^[a-z0-9]+(-[a-z0-9]+)*$
```

Skill names are typed on the command line, used as directory names, and referenced from plugin manifests, so a single portable casing convention avoids case-sensitivity and quoting surprises across platforms.

This rule is stricter than [skill/name-invalid-chars](name-invalid-chars.md), which only rejects characters outside the engine's permitted set.

## Examples

### Incorrect

```markdown
---
name: MySkill
description: Uppercase letters are not kebab-case
---
```

```markdown
---
name: my_skill
description: Underscores are not hyphens
---
```

```markdown
---
name: my skill
description: Spaces are not hyphens
---
```

```markdown
---
name: my--skill
description: Doubled hyphens leave an empty segment
---
```

### Correct

```markdown
---
name: my-skill
description: Does something useful
---
```

```markdown
---
name: pdf-2-text
description: Digits are allowed inside segments
---
```

## How to fix

Lowercase the name and replace every separator (spaces, underscores, dots, camelCase boundaries) with a single hyphen. Drop leading and trailing hyphens.

## See also

- [skill/name-invalid-chars](name-invalid-chars.md) — validates the engine-permitted character set
- [skill/name-too-long](name-too-long.md) — validates the name length limit
- [Using `aipm lint`](../../guides/lint.md) — CLI reference for running the lint system
