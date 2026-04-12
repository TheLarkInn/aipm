# source/misplaced-features

**Severity:** warning
**Fixable:** No

Checks that plugin feature files (skills, agents, hooks, commands, output styles, and
extensions) are located inside the `.ai/` marketplace directory rather than in tool-specific
locations such as `.claude/` or `.github/`. Files in legacy locations are not discovered,
installed, or linked by `aipm`.

This rule fires regardless of whether a `.ai/` directory exists. The fix guidance adapts
based on your project state (see [How to fix](#how-to-fix) below).

## Exempt files

**Instruction files are always exempt from this rule.** The following file types live at
the repository root (or within tool directories) by design — they are AI context files,
not plugin features — and are never flagged as misplaced:

| File pattern | Examples |
|---|---|
| `CLAUDE.md` | Anthropic Claude project instructions |
| `AGENTS.md` | OpenAI Codex / o1 agent instructions |
| `COPILOT.md` | GitHub Copilot instructions |
| `GEMINI.md` | Google Gemini instructions |
| `INSTRUCTIONS.md` | Generic instructions file |
| `*.instructions.md` | VS Code Copilot scoped instructions (e.g. `python.instructions.md`) |

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

### Correct — plugin features in `.ai/`
```
.ai/
  my-plugin/
    SKILL.md      # ✅ discovered and managed by aipm
```

### Correct — instruction file at repo root (exempt)
```
CLAUDE.md         # ✅ exempt — instruction file, not a plugin feature
AGENTS.md         # ✅ exempt — instruction file, not a plugin feature
.github/
  copilot-instructions.md   # ✅ exempt — *.instructions.md pattern
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

## See also

- [Migrating existing configurations](../../guides/migrate.md) — full `aipm migrate` reference, including `--dry-run`
- [Migrating — step-by-step](../../guides/migrating-existing-configs.md) — dry-run, destructive cleanup, and recursive discovery walkthrough
- [Creating a plugin](../../guides/creating-a-plugin.md) — scaffold a new plugin directly in `.ai/`
- [Using `aipm lint`](../../guides/lint.md) — CLI reference for running the lint system
