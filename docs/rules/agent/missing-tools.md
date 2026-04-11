# agent/missing-tools

**Severity:** warning
**Fixable:** No

Checks that every agent `.md` file in the `agents/` directory includes a `tools` field in the YAML frontmatter. Declaring required tools allows the runtime to validate availability before invoking the agent and helps users understand its dependencies.

## Examples

### Incorrect
```markdown
---
name: my-agent
description: Automates a workflow
---
Agent instructions here...
```

### Correct
```markdown
---
name: my-agent
description: Automates a workflow
tools:
  - bash
  - read_file
---
Agent instructions here...
```

## How to fix
Add a `tools` field to the YAML frontmatter listing each tool the agent requires. Use a YAML sequence (one tool per line with a leading `-`).

## See also

- [Creating a plugin](../../guides/creating-a-plugin.md) — how to scaffold a new plugin with correctly structured agent files
- [Using `aipm lint`](../../guides/lint.md) — CLI reference for running the lint system
- [Configuring lint](../../guides/configuring-lint.md) — override rule severity or suppress rules per path
