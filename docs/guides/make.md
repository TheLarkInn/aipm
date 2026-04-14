# Scaffolding a Plugin with `aipm make plugin`

`aipm make plugin` creates a new AI plugin **inside an existing workspace's `.ai/` marketplace directory**. It wires everything up in one step: creates the plugin directory, writes feature-appropriate templates, generates `plugin.json`, registers the plugin in `marketplace.json`, and updates engine settings.

> **`aipm make` vs `aipm-pack init`**
>
> | Command | What it does |
> |---------|--------------|
> | `aipm make plugin` | Creates a plugin *inside* an existing `.ai/` workspace (run inside an initialised project) |
> | `aipm-pack init` | Scaffolds a standalone plugin *package* for publishing to a registry |
>
> If you want to add a plugin to your current project, use `aipm make plugin`.  
> If you want to create a redistributable plugin package, see [`docs/guides/creating-a-plugin.md`](./creating-a-plugin.md).

## Prerequisites

Your project must already be initialised with `aipm init`. If you haven't done that yet:

```bash
aipm init
```

## Basic Usage

```bash
# Interactive wizard (prompts for name, engine, and features)
aipm make plugin

# Fully non-interactive
aipm make plugin --name my-plugin --engine claude --feature skill -y
```

## CLI Reference

```
aipm make plugin [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `--name <NAME>` | Plugin name (prompted if omitted on a TTY) |
| `--engine <ENGINE>` | Target engine: `claude` (default), `copilot`, or `both` |
| `--feature <FEATURE>` | Feature type to include (repeatable). Prompted if omitted on a TTY. |
| `-y, --yes` | Skip interactive prompts and use defaults |
| `--dir <DIR>` | Project directory (default: `.`) |

## Supported Features

The available `--feature` values depend on the target `--engine`:

| Feature | CLI flag | Claude | Copilot |
|---------|----------|--------|---------|
| Skills (prompt templates) | `skill` | ✓ | ✓ |
| Agents (autonomous sub-agents) | `agent` | ✓ | ✓ |
| MCP Servers (tool providers) | `mcp` | ✓ | ✓ |
| Hooks (lifecycle events) | `hook` | ✓ | ✓ |
| Output Styles (response formatting) | `output-style` | ✓ | — |
| LSP Servers (language intelligence) | `lsp` | — | ✓ |
| Extensions (Copilot extensions) | `extension` | — | ✓ |

Use `--engine both` to scaffold a plugin that targets all supported engines, which enables all seven feature types.

## Examples

### Skill plugin for Claude

```bash
aipm make plugin --name my-skill --engine claude --feature skill -y
```

Creates:

```
.ai/
  my-skill/
    skills/
      my-skill/
        SKILL.md          # lint-passing template with name + description
    .claude-plugin/
      plugin.json
  .claude-plugin/
    marketplace.json      # updated to register my-skill
.claude/
  settings.json           # updated with enabledPlugins
```

### Agent plugin for Copilot

```bash
aipm make plugin --name my-agent --engine copilot --feature agent -y
```

### Composite plugin with multiple features

```bash
aipm make plugin --name my-toolkit --engine claude \
  --feature skill \
  --feature agent \
  --feature hook \
  -y
```

### Cross-engine plugin

```bash
aipm make plugin --name shared-tools --engine both \
  --feature skill \
  --feature agent \
  --feature mcp \
  -y
```

## What Gets Created

For each `--feature`, the command creates a feature-specific subdirectory and a lint-passing template file:

| Feature | Directory | Template file |
|---------|-----------|---------------|
| `skill` | `skills/<plugin-name>/` | `SKILL.md` — frontmatter with `name` and `description` |
| `agent` | `agents/` | `<plugin-name>.md` — frontmatter with `name`, `description`, and `tools` |
| `mcp` | (root) | `.mcp.json` — empty `mcpServers` object |
| `hook` | `hooks/` | `hooks.json` — empty `hooks` array |
| `output-style` | `output-styles/` | `<plugin-name>.md` — minimal output style template |
| `lsp` | `lsp/` | `<plugin-name>.json` — minimal LSP configuration |
| `extension` | `extensions/` | `<plugin-name>.json` — minimal extension configuration |

All generated templates pass `aipm lint` with zero errors out of the box.

## Idempotency

`aipm make plugin` is idempotent. If the plugin directory already exists, the command exits early and reports `"<name> already exists"`. Individual files that already exist are also skipped rather than overwritten.

## After Creating a Plugin

Once your plugin is scaffolded, you can:

- Edit the generated templates to add your actual content.
- Run `aipm lint` to validate the plugin against all quality rules.
- Use `aipm list` to confirm the plugin is registered in your workspace.
- Commit the new plugin files and share them with your team.

## See also

- [`aipm init`](./init.md) — initialise a workspace before running `aipm make plugin`
- [`docs/guides/creating-a-plugin.md`](./creating-a-plugin.md) — create a redistributable plugin package with `aipm-pack init`
- [`docs/guides/lint.md`](./lint.md) — lint your newly created plugin
- [`docs/rules/`](../rules/) — lint rule reference
