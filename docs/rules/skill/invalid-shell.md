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
