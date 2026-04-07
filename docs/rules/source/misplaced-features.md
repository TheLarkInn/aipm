# source/misplaced-features

**Severity:** warning
**Fixable:** No

Checks that plugin feature files are located inside the `.ai/` marketplace directory rather than in legacy locations such as `.claude/` or `.github/`. Misplaced files are not discovered by aipm and will not be installed or linked.

The rule triggers when any of the following subdirectories are found inside `.claude/` or `.github/`:

| Directory | Content |
|---|---|
| `skills/` | Skill definitions (`SKILL.md`) |
| `commands/` | Slash-command prompts |
| `agents/` | Agent definitions |
| `hooks/` | Hook configuration (`hooks.json`) |
| `output-styles/` | Output style definitions |
| `extensions/` | Tool extensions |

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

## How to fix
Run `aipm migrate` to automatically move feature files from legacy locations into the `.ai/` marketplace directory with the correct structure.
