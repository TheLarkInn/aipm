# Creating a Plugin with `aipm make plugin`

`aipm make plugin` scaffolds a new plugin directly inside your workspace's existing `.ai/` marketplace. Use it when you want to add a plugin to a project that was already initialized with `aipm init`.

> **Tip:** If you want to create a standalone, publishable plugin package instead, use [`aipm-pack init`](creating-a-plugin.md).

## Quick start

```bash
# Interactive wizard (TTY)
aipm make plugin

# Non-interactive (all required flags supplied)
aipm make plugin --name my-skill --feature skill
```

## Synopsis

```
aipm make plugin [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `--name <NAME>` | Plugin name (required in non-interactive mode) |
| `--engine <ENGINE>` | Target engine: `claude` (default), `copilot`, or `both` |
| `--feature <FEATURE>` | Feature type to include (repeatable; required in non-interactive mode) |
| `-y, --yes` | Skip interactive prompts, accept defaults |
| `--dir <DIR>` | Project directory (default: `.`) |

## Available features

Features are filtered by the chosen engine. Pass `--feature` once per type you want to scaffold.

| Feature (`--feature` value) | Description | Claude | Copilot |
|------------------------------|-------------|:------:|:-------:|
| `skill` | Prompt templates (`SKILL.md`) | ✓ | ✓ |
| `agent` | Autonomous sub-agents | ✓ | ✓ |
| `mcp` | MCP server configuration | ✓ | ✓ |
| `hook` | Lifecycle event hooks | ✓ | ✓ |
| `output-style` | Response formatting rules | ✓ | — |
| `lsp` | Language Server integration | — | ✓ |
| `extension` | Copilot extensions | — | ✓ |

Specifying `--engine both` makes all seven feature types available.

## How it works

`aipm make plugin` runs an **idempotent 9-step action pipeline**:

1. Guard — returns immediately if the plugin directory already exists (outputs `Already exists: <path>`).
2. Create the plugin directory (`.ai/<marketplace>/<name>/`).
3. Create the `.claude-plugin/` metadata subdirectory.
4. Scaffold each requested feature (creates subdirectories and starter files).
5. Generate and write `plugin.json`.
6. Register the plugin in `marketplace.json`.
7. Update `.claude/settings.json` — only when `--engine` is `claude` or `both`; no settings file is written for `--engine copilot`.
8. Emit a summary `PluginCreated` action.

Each step is tracked as an `Action` variant. Re-running the command on an existing plugin directory is safe — it exits at step 1 with `Already exists: <path>` and makes no further changes.

## Non-interactive usage

In non-interactive mode (`--yes` or a non-TTY environment), both `--name` and at least one `--feature` flag are required:

```bash
# Single feature
aipm make plugin --name code-review --feature skill

# Multiple features
aipm make plugin --name dev-tools --engine claude \
  --feature skill --feature agent --feature hook

# Target Copilot
aipm make plugin --name ide-helper --engine copilot \
  --feature skill --feature lsp

# Scaffold for both engines
aipm make plugin --name shared-kit --engine both \
  --feature skill --feature agent --feature mcp

# Run in a different directory
aipm make plugin --name my-plugin --feature skill --dir /path/to/project
```

## Interactive wizard

When run on a TTY without `--yes`, the wizard runs in two phases:

**Phase 1 — Name & engine** (skipped if supplied via flags):
1. Plugin name prompt (lowercase, hyphens allowed)
2. Target engine select (`claude` / `copilot` / `both`)

**Phase 2 — Features** (skipped if `--feature` flags are supplied):
- Multi-select list filtered to features supported by the chosen engine

## What gets created

For `aipm make plugin --name my-skill --engine claude --feature skill`:

```
.ai/<marketplace>/
  my-skill/
    .claude-plugin/
      plugin.json          # plugin metadata
    skills/
      SKILL.md             # starter skill template
  .claude-plugin/
    marketplace.json       # updated to include my-skill
```

For `--engine copilot --feature skill --feature lsp`:

```
.ai/<marketplace>/
  my-plugin/
    .claude-plugin/
      plugin.json
    skills/
      SKILL.md
    .lsp.json              # LSP server config (root of plugin directory)
```

Claude engine settings (`.ai/.claude/settings.json`) are updated automatically. Copilot settings support is deferred to a future release.

## Discovery

`aipm make plugin` walks up the directory tree from `--dir` (default: `.`) to find the nearest `.ai/` marketplace directory. If no marketplace is found, the command exits with an error — run `aipm init` first to create one.

## Relationship to other commands

| Command | What it does |
|---------|--------------|
| `aipm init` | Creates the workspace and empty marketplace |
| `aipm make plugin` | Adds a new plugin **inside** an existing marketplace |
| `aipm-pack init` | Creates a standalone, publishable plugin **package** |
| `aipm install` | Installs a plugin from a registry, git, or local path |

## See also

- [`docs/guides/init.md`](init.md) — workspace and marketplace setup
- [`docs/guides/creating-a-plugin.md`](creating-a-plugin.md) — authoring a publishable package with `aipm-pack init`
- [`docs/guides/local-development.md`](local-development.md) — iterating on a plugin locally with `aipm link`
