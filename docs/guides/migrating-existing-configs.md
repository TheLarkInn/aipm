# Migrating Existing Configurations

`aipm migrate` converts existing AI tool configurations (`.claude/`, `.github/`, etc.) into marketplace plugins inside your `.ai/` directory. It discovers skills, agents, MCP servers, hooks, commands, and output styles, then copies them into a structured plugin layout.

## Quick start

```bash
# 1. Preview what will be migrated (no files written)
aipm migrate --dry-run

# 2. Run the migration
aipm migrate
```

When run interactively on a TTY, `aipm migrate` asks whether to remove the original source files after a successful migration.

## Dry-run preview

Always start with `--dry-run` to understand what will change:

```bash
aipm migrate --dry-run
```

The report lists every detected artifact, the plugin it will be grouped into, and whether source files will be removed. No files are written.

## Artifact types

`aipm migrate` detects artifacts from two source ecosystems:

### Claude Code (`.claude/`)

| Artifact | Source location | Plugin type |
|----------|----------------|-------------|
| Skills | `.claude/skills/<name>/SKILL.md` | `skill` |
| Commands | `.claude/commands/<name>.md` | `skill` |
| Agents | `.claude/agents/<name>.md` | `agent` |
| MCP servers | `.mcp.json` | `mcp` |
| Hooks | `.claude/settings.json` hooks block | `hook` |
| Output styles | `.claude/output-styles/<name>.md` | `composite` |
| LSP servers | `lsp.json` | `lsp` |

### Copilot CLI (`.github/`)

| Artifact | Source location | Plugin type |
|----------|----------------|-------------|
| Skills | `.github/skills/<name>/` | `skill` |
| Agents | `.github/agents/<name>.md` or `<name>.agent.md` | `agent` |
| MCP servers | `.copilot/mcp-config.json` | `mcp` |
| Hooks | `.github/hooks.json` or `.github/hooks/hooks.json` | `hook` |
| GitHub extensions | `.github/extensions/<name>/` | `composite` |
| LSP servers | `.github/lsp.json` or `lsp.json` | `lsp` |

## Common flags

```
aipm migrate [OPTIONS] [DIR]
```

| Flag | Description |
|------|-------------|
| `--dry-run` | Preview migration without writing files |
| `--destructive` | Remove source files after migration (non-interactive) |
| `--source <SRC>` | Limit to a specific source directory (e.g., `.claude`) |
| `--max-depth <N>` | Maximum depth for recursive source discovery |
| `--manifest` | Generate `aipm.toml` plugin manifests for each migrated plugin |

## Recursive discovery

By default, `aipm migrate` recursively discovers all `.claude/` and `.github/` directories under the target directory. This covers monorepos with multiple project roots.

```bash
# Migrate all .claude/ and .github/ directories found anywhere under the repo
aipm migrate

# Limit to a specific source only
aipm migrate --source .claude
aipm migrate --source .github
```

Use `--max-depth` to limit how deep the recursive scan goes:

```bash
aipm migrate --max-depth 3
```

## Removing source files

After a successful migration you can optionally remove the original source files.

**Interactive (default on TTY):** `aipm migrate` prompts before removing files.

**Non-interactive:** pass `--destructive` to remove files automatically.

```bash
# Always prompt (default)
aipm migrate

# Always remove after migration without prompting
aipm migrate --destructive
```

> **Tip:** Combine with `--dry-run` first to verify output before passing `--destructive`.

## Generating plugin manifests

Pass `--manifest` to also create an `aipm.toml` manifest for each migrated plugin:

```bash
aipm migrate --manifest
```

This generates a manifest with the plugin `name`, `version`, `type`, and `files` fields derived from what was detected. You can edit the manifest afterwards to add `engines`, `description`, and `[dependencies]`.

## Example workflow

```bash
# 1. Preview
aipm migrate --dry-run

# 2. Migrate and create manifests, then clean up originals
aipm migrate --manifest --destructive

# 3. Lint to verify plugin quality
aipm lint

# 4. Commit
git add .ai/
git commit -m "chore: migrate .claude configs to .ai marketplace plugins"
```

## Output layout

After migration, each detected artifact appears under `.ai/` as a named plugin directory:

```
.ai/
  my-skill/
    skills/
      my-skill/
        SKILL.md
    aipm.toml     # present with --manifest
  my-agent/
    agents/
      my-agent.md
    aipm.toml
```

See also: [`aipm migrate`](../../README.md#aipm-migrate), [`aipm lint`](../../README.md#aipm-lint).
