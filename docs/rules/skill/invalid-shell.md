# skill/invalid-shell

**Severity:** error
**Fixable:** No

Checks that the `shell` field in SKILL.md frontmatter, when present, contains a recognised shell identifier. Unsupported shell values may cause the skill to fail at runtime.

## Supported values

| Value | Description |
|---|---|
| `bash` | GNU Bash (default on Linux/macOS) |
| `powershell` | Windows PowerShell |

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

## How to fix
Use one of the supported shell values: `bash` or `powershell`. Remove the `shell` field entirely to fall back to the runtime default (`bash`).
