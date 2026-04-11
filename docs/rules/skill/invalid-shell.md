# skill/invalid-shell

**Severity:** error
**Fixable:** No

Checks that the `shell` field in SKILL.md frontmatter, when present, contains a recognised shell identifier. Unsupported shell values may cause the skill to fail at runtime.

## Examples

### Incorrect

```markdown
---
name: my-skill
description: Does something useful
shell: zsh
---
```

### Correct

```markdown
---
name: my-skill
description: Does something useful
shell: bash
---
```

```markdown
---
name: my-skill
description: Does something useful
shell: powershell
---
```

## How to fix

Use one of the two supported shell values: `bash` or `powershell`. Remove the `shell` field entirely to fall back to the runtime default.

## Supported values

| Value | Platform |
|-------|----------|
| `bash` | Linux / macOS |
| `powershell` | Windows |

## See also

- [Engine & Platform Compatibility](../../guides/engine-platform-compatibility.md) — declare supported AI tools and operating systems in `aipm.toml`
- [Creating a plugin](../../guides/creating-a-plugin.md) — how to scaffold a new plugin
- [Using `aipm lint`](../../guides/lint.md) — CLI reference for running the lint system
