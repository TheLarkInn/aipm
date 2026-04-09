# source/misplaced-features

**Severity:** warning
**Fixable:** No

Checks that plugin feature files (skills, agents, hooks, commands, output styles, and
extensions) are located inside the `.ai/` marketplace directory rather than in tool-specific
locations such as `.claude/` or `.github/`. Files in legacy locations are not discovered,
installed, or linked by `aipm`.

This rule fires regardless of whether a `.ai/` directory exists. The fix guidance adapts
based on your project state (see [How to fix](#how-to-fix) below).

## Examples

### Incorrect — skill in `.claude/`
```
.claude/
  skills/
    my-skill.md   # ❌ not discovered by aipm
```

### Incorrect — agent in `.github/`
```
.github/
  agents/
    my-agent.md   # ❌ not discovered by aipm
```

### Correct
```
.ai/
  my-plugin/
    SKILL.md      # ✅ discovered and managed by aipm
```

## How to fix

The fix depends on whether a `.ai/` marketplace directory already exists in your project.

### No `.ai/` directory yet

First create the marketplace, then migrate your existing configs:

```bash
aipm init          # scaffolds .ai/ and registers it with your AI tools
aipm migrate       # moves feature files from .claude/ / .github/ into .ai/
```

### `.ai/` directory already exists

Run migration to move the misplaced files:

```bash
aipm migrate       # moves feature files into the .ai/ marketplace
```

Use `--dry-run` to preview the migration plan before writing any files:

```bash
aipm migrate --dry-run
```

See [Migrating Existing Configurations](../../guides/migrate.md) for a full reference.
