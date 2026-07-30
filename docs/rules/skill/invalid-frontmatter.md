# skill/invalid-frontmatter

**Severity:** error
**Fixable:** No

Validates the *structure* of a `SKILL.md` frontmatter block. Both Claude Code and Copilot CLI extract frontmatter with an anchored `---` match and then hand the block to a YAML parser — if any of the checks below fail, the engine silently ignores the file and the skill never loads.

This rule reports:

| Check | Failure message |
|---|---|
| The file starts **immediately** with a line containing exactly `---` | `file must start immediately with a line containing exactly ---` |
| A UTF-8 byte-order mark precedes the opening delimiter | `... (a UTF-8 byte-order mark precedes it)` |
| The block is closed by a line containing exactly `---` | `frontmatter is not closed by a line containing exactly ---` |
| The block between the delimiters is valid YAML | `frontmatter is not valid YAML (line N): ...` |
| The YAML root is a key/value mapping | `frontmatter must be a YAML mapping of keys to values` |
| `name` parses to a YAML string | `SKILL.md field "name" must be a string, found <type>` |
| `description` parses to a YAML string | `SKILL.md field "description" must be a string, found <type>` |

Both LF and CRLF line endings are accepted. Delimiters must be *exact*: `----`, `--- `, or an indented `---` are all rejected.

## Examples

### Incorrect

A blank line before the frontmatter:

```markdown

---
name: my-skill
description: Does something useful
---
```

An inexact closing delimiter:

```markdown
---
name: my-skill
description: Does something useful
----
```

Invalid YAML:

```markdown
---
name: my-skill
tags: [unclosed
---
```

A root that is not a mapping:

```markdown
---
- my-skill
- Does something useful
---
```

A non-string field:

```markdown
---
name: 42
description:
  - one
  - two
---
```

### Correct

```markdown
---
name: my-skill
description: Does something useful
---

# My Skill
```

Folded and literal block scalars are valid — they parse to strings:

```markdown
---
name: my-skill
description: >
  A long description that wraps
  across several source lines.
---
```

## How to fix

1. Remove anything (including blank lines and byte-order marks) before the opening `---`.
2. Make both delimiter lines contain exactly three hyphens and nothing else.
3. Fix the YAML syntax reported in the message — the line number is file-relative.
4. Make sure the block is a mapping of keys to values, and that `name` and `description` are strings (quote them if they would otherwise parse as a number or boolean).

## See also

- [skill/missing-name](missing-name.md) — validates that a `name` field is present
- [skill/missing-description](missing-description.md) — validates that a `description` field is present and non-empty
- [Using `aipm lint`](../../guides/lint.md) — CLI reference for running the lint system
