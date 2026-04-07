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
shell: powershell
---
```

### Correct — bash
```markdown
---
name: my-skill
description: Does something useful
shell: bash
---
```

### Correct — powershell
```markdown
---
name: my-skill
description: Does something useful
shell: powershell
---
```

## Supported values

| Value | Runtime |
|-------|---------|
| `bash` | Bash shell (Linux, macOS) |
| `powershell` | PowerShell (Windows) |

## How to fix
Set `shell` to `bash` or `powershell`, or remove the field entirely to fall back to the runtime default.
