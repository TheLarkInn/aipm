# skill/invalid-shell

**Severity:** warning
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

### Correct
```markdown
---
name: my-skill
description: Does something useful
shell: bash
---
```

## How to fix
Use a supported shell value such as `bash`, `sh`, or `zsh`. Remove the `shell` field entirely to fall back to the runtime default, or consult the aipm documentation for the full list of supported values.
