# skill/name-invalid-chars

**Severity:** warning
**Fixable:** No

Checks that the `name` field in SKILL.md frontmatter matches the Copilot CLI pattern `/^[a-zA-Z0-9][a-zA-Z0-9._\- ]*$/`:

- The **first character** must be alphanumeric (`a–z`, `A–Z`, `0–9`).
- Subsequent characters may be alphanumeric, a dot (`.`), an underscore (`_`), a hyphen (`-`), or a space (` `).

Any other character (e.g. `!`, `@`, `/`) triggers this rule.

> **Tip:** Although spaces are technically allowed by the pattern, prefer hyphens (`-`) as word separators. Spaces can cause quoting issues in shell integration and make names harder to type.

## Examples

### Incorrect
```markdown
---
name: my-skill!
description: Does something useful
---
```

```markdown
---
name: /absolute/path
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

```markdown
---
name: my skill
description: Space is technically allowed, but hyphens are preferred.
---
```

## How to fix
Remove any characters that are not alphanumeric, dots, underscores, hyphens, or spaces. Ensure the name starts with an alphanumeric character. Replace spaces with hyphens for maximum portability.
