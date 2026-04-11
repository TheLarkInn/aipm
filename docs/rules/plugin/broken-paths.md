# plugin/broken-paths

**Severity:** error
**Fixable:** No

Checks that every script or file path referenced via `${CLAUDE_SKILL_DIR}/` or `${SKILL_DIR}/` variables inside a `SKILL.md` file resolves to an existing file on disk. Broken references cause the skill to fail at runtime when the AI agent attempts to invoke the script.

This rule also **silently rejects** two unsafe reference patterns for security:

- **Absolute paths** (starting with `/`) — rejected to prevent escaping the plugin directory.
- **Path traversal sequences** (containing `..`) — rejected to prevent accessing files outside the skill directory.

## Examples

### Incorrect

```markdown
---
name: my-skill
description: Runs a deployment script
---
To deploy, run `${CLAUDE_SKILL_DIR}/scripts/deploy.sh`.
```

*(where `scripts/deploy.sh` does not exist relative to the `SKILL.md` file)*

### Correct

```markdown
---
name: my-skill
description: Runs a deployment script
---
To deploy, run `${CLAUDE_SKILL_DIR}/scripts/deploy.sh`.
```

*(where `scripts/deploy.sh` exists alongside the `SKILL.md` file)*

## How to fix

Either create the missing script at the referenced path relative to the `SKILL.md` file, update the reference to point to the correct existing file, or remove the broken reference from the skill body entirely.

## Variable prefixes recognised

| Variable | Meaning |
|---|---|
| `${CLAUDE_SKILL_DIR}/` | Directory containing the `SKILL.md` file (Claude Code convention) |
| `${SKILL_DIR}/` | Alias for the same directory (portable convention) |

## See also

- [plugin/missing-manifest](missing-manifest.md) — validates that `.claude-plugin/plugin.json` exists
- [plugin/required-fields](required-fields.md) — validates that `plugin.json` contains all required fields
- [Creating a plugin](../../guides/creating-a-plugin.md) — how to scaffold a new plugin with the correct directory layout
- [Using `aipm lint`](../../guides/lint.md) — CLI reference for running the lint system
