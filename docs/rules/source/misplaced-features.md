# source/misplaced-features

**Severity:** warning
**Fixable:** No

Checks that plugin feature files (skills, agents, hooks) are located inside the `.ai/` marketplace directory rather than in legacy locations such as `.claude/` or `.github/`. Misplaced files are not discovered by aipm and will not be installed or linked.

## Examples

### Incorrect
```
.claude/
  skills/
    my-skill.md   # not discovered by aipm
```

### Correct
```
.ai/
  my-plugin/
    SKILL.md      # discovered and managed by aipm
```

## Triggered directories

The rule fires when feature files are found under any of the following subdirectory names inside a non-`.ai/` source directory (e.g., `.claude/` or `.github/`):

| Directory | Feature type |
|-----------|-------------|
| `skills/` | Skill plugins (`SKILL.md`) |
| `commands/` | Command plugins (treated as a skill subtype) |
| `agents/` | Agent plugins |
| `hooks/` | Hook plugins (`hooks.json`) |
| `output-styles/` | Output style plugins |
| `extensions/` | Extension plugins |

## How to fix
Run `aipm migrate` to automatically move feature files from legacy locations into the `.ai/` marketplace directory with the correct structure.
