# Initializing a Workspace (`aipm init`)

`aipm init` bootstraps a repository for AI plugin management. It creates a `.ai/`
local marketplace directory with a starter plugin, writes tool-specific settings
so AI coding tools (Claude Code, Copilot, etc.) discover the marketplace automatically,
and optionally generates a workspace `aipm.toml` manifest for dependency tracking.

## Quick start

```bash
# Scaffold a marketplace in the current directory (interactive on a TTY)
aipm init

# Non-interactive with all defaults
aipm init --yes

# Full workspace setup: manifest + marketplace + starter plugin
aipm init --workspace --marketplace
```

Run `aipm init` once per repository. Re-running in an already-initialized directory
fails with `already initialized` to protect existing configuration.

## What gets created

Running `aipm init` (the default, no-flag invocation) creates:

```
.ai/
  .gitignore                              # aipm-managed block + .tool-usage.log
  .claude-plugin/
    marketplace.json                      # registers the local marketplace
  starter-aipm-plugin/
    .claude-plugin/
      plugin.json                         # Claude Code plugin manifest
    skills/
      scaffold-plugin/
        SKILL.md                          # example skill
    agents/
      marketplace-scanner.md             # example agent
    hooks/
      hooks.json                          # example hooks
    scripts/
      scaffold-plugin.sh                  # scaffold helper script
    .mcp.json                             # MCP server stub
.claude/
  settings.json                           # registers the marketplace + enables starter plugin
```

Adding `--workspace` also creates:

```
aipm.toml                                 # workspace manifest with [workspace] section
```

## CLI flags

| Flag | Description |
|------|-------------|
| `-y, --yes` | Skip interactive prompts; accept all defaults |
| `--workspace` | Generate a workspace `aipm.toml` manifest (with `[workspace]` section) |
| `--marketplace` | Create the `.ai/` marketplace directory and tool settings |
| `--no-starter` | Omit the starter plugin; create a bare `.ai/` directory only |
| `--manifest` | Generate an `aipm.toml` plugin manifest for the starter plugin (opt-in) |
| `--name <NAME>` | Custom marketplace name (default: `local-repo-plugins`) |
| `DIR` | Target directory (default: current directory) |

## Initialization modes

### Default: marketplace only

```bash
aipm init
```

Creates `.ai/`, the starter plugin, `marketplace.json`, and tool settings. Does **not**
create a workspace `aipm.toml`. Use this when you only need local plugins managed
by `aipm lint` and `aipm migrate`, without the lockfile-based dependency system.

### Workspace manifest only

```bash
aipm init --workspace
```

Creates `aipm.toml` with a `[workspace]` section (pointing at `.ai/*` members) but
skips the marketplace directory. Use this when you already have a `.ai/` directory or
want to set up the manifest before populating the marketplace.

### Full setup

```bash
aipm init --workspace --marketplace
```

Creates both the workspace manifest and the full marketplace. This is the recommended
setup for new repositories that will use `aipm install` and `aipm lint`.

### Bare marketplace (no starter plugin)

```bash
aipm init --no-starter
```

Scaffolds `.ai/` and writes tool settings but skips the starter plugin. The `.ai/`
directory is empty (aside from `.gitignore` and `marketplace.json`). Use this when
you plan to add plugins immediately via `aipm install` or `aipm migrate` and don't
want the boilerplate starter.

`--no-starter` can be combined with `--workspace` and `--marketplace`:

```bash
aipm init --workspace --marketplace --no-starter
```

## Non-interactive usage

When `--yes` is passed (or when stdin is not a TTY), all interactive prompts are
skipped and defaults are used. Combine with explicit flags for fully automated
initialization in CI or scripts:

```bash
# Automated full setup in a CI environment
aipm init --yes --workspace --marketplace --name my-org-plugins
```

## Custom marketplace name

By default the marketplace is named `local-repo-plugins`. Use `--name` to override:

```bash
aipm init --name @acme/ai-plugins
```

The name appears in `marketplace.json`, `aipm.toml` members glob, and the
`enabledPlugins` entry in `settings.json` (e.g., `starter-aipm-plugin@acme-ai-plugins`).

## Plugin manifests (opt-in)

By default `aipm init` does not create `aipm.toml` plugin manifests inside `.ai/`.
Pass `--manifest` to generate one for the starter plugin:

```bash
aipm init --marketplace --manifest
```

This produces `.ai/starter-aipm-plugin/aipm.toml` with `name`, `version = "0.1.0"`,
and `type = "composite"`. The manifest is required if you intend to publish the starter
plugin to a registry or track it with lockfile-based dependency management.

## Tool settings integration

`aipm init` writes or merges tool-specific configuration files so that AI coding tools
discover the `.ai/` marketplace automatically. Existing settings files are preserved — `aipm`
only adds missing keys; it never overwrites or removes existing configuration.

### Claude Code

`aipm init` creates or merges `.claude/settings.json`:

```json
{
  "extraKnownMarketplaces": [
    { "type": "directory", "directory": "./.ai" }
  ],
  "enabledPlugins": ["starter-aipm-plugin@local-repo-plugins"]
}
```

After initialization, Claude Code picks up the marketplace on the next restart. Open any
skill, agent, hook, or command file under `.ai/` to confirm the integration is active.

## Error conditions

| Error message | Cause | Fix |
|---|---|---|
| `already initialized` | `aipm.toml` already exists in the target directory | Remove the manifest or choose a different directory |
| `already exists` | `.ai/` directory already exists | Use `--no-starter` to extend a bare `.ai/`, or run `aipm migrate` to populate an existing one |

## Next steps

After initializing, you can:

- **Add more plugins** — `aipm install` to install from a registry, or `aipm migrate` to
  convert existing `.claude/` / `.github/` configurations
- **Lint your plugins** — `aipm lint` to check the starter plugin and any installed plugins
  for quality issues
- **Develop locally** — edit files under `.ai/starter-aipm-plugin/` and use `aipm link` /
  `aipm unlink` for local overrides
- **Create a publishable plugin** — run `aipm pack init` in a new directory to scaffold a
  standalone plugin package

## See also

- [Creating a plugin](creating-a-plugin.md) — scaffold a standalone plugin for publishing with `aipm pack init`
- [Migrating Existing Configurations](migrate.md) — convert `.claude/` / `.github/` configs after `aipm init`
- [Configuring Lint](configuring-lint.md) — tune rule severity and configure editor schema support
- [VS Code Extension](vscode-extension.md) — real-time lint diagnostics in VS Code via `aipm lsp`
