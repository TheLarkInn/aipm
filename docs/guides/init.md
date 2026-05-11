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

**`aipm init` is idempotent.** Re-running it in an already-initialized directory is safe —
pre-existing artifacts (`aipm.toml`, `.ai/`, marketplace manifests) are detected and
reused rather than overwritten, and the command reports what it found. Only new files
that are missing from a previous partial run are created.

## What gets created

Running `aipm init` (default, no-flag invocation) with the default **copilot**
engine creates:

```
.ai/
  .gitignore                              # aipm-managed block + .tool-usage.log
  .claude-plugin/
    marketplace.json                      # registers the local marketplace
  starter-aipm-plugin/
    .claude-plugin/
      plugin.json                         # plugin manifest
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
.github/
  copilot-instructions.md                 # Copilot marketplace pointer (copilot engine)
```

Using `--engine claude` produces `.claude/settings.json` instead of
`.github/copilot-instructions.md`. Using `--engine claude,copilot` produces both.

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
| `--engine <LIST>` | Engines to scaffold for, comma-separated (e.g. `claude`, `copilot`, or `claude,copilot`). Repeated flags are merged. Omit to let the wizard prompt. |
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

## Engine-aware scaffolding

`aipm init` creates tool-specific configuration files only for the engines you
are actually using. Use `--engine` to control which engine adaptors run:

| Command | Files created |
|---------|--------------|
| `aipm init --engine claude` | `.claude/settings.json` only |
| `aipm init --engine copilot` | `.github/copilot-instructions.md` only |
| `aipm init --engine claude,copilot` | Both — `.claude/settings.json` **and** `.github/copilot-instructions.md` |

Repeated `--engine` flags are merged, so `--engine claude --engine copilot` is
equivalent to `--engine claude,copilot`.

When run non-interactively (`--yes`) without `--engine`, the default is
**copilot** (Copilot CLI). The interactive wizard always prompts for your
preferred engine.

### Copilot CLI scaffold

`--engine copilot` creates `.github/copilot-instructions.md` instead of
`.claude/settings.json`. This file registers the `.ai/` marketplace as a Copilot
instruction source:

```
.github/
  copilot-instructions.md    # Marketplace pointer + starter-plugin section
```

Example content (with the starter plugin):

```markdown
# Copilot Instructions

This project uses [aipm](https://github.com/TheLarkInn/aipm) to manage AI plugins.
The local marketplace lives at `.ai/` and is registered as `local-repo-plugins`.

## Default plugin

This project bundles the `starter-aipm-plugin@local-repo-plugins` plugin.

<!-- aipm marketplace pointer; do not edit between markers -->
<!-- AIPM_MARKETPLACE: local-repo-plugins -->
```

Existing `copilot-instructions.md` files are left untouched — `aipm` never
overwrites user-managed content.

### Claude Code scaffold

`--engine claude` creates or merges `.claude/settings.json` (unchanged from
earlier behavior):

```json
{
  "extraKnownMarketplaces": [
    { "type": "directory", "directory": "./.ai" }
  ],
  "enabledPlugins": ["starter-aipm-plugin@local-repo-plugins"]
}
```

### Declaring engine support in `aipm.toml`

The `--engine` flag controls *which adaptors scaffold*; it does not
automatically write an `engines` field to `aipm.toml`. To declare which
engines the workspace plugins are compatible with, add the optional
`engines` field manually (or via `--workspace`):

```toml
[workspace]
members = [".ai/*"]
engines = ["claude", "copilot"]   # omit to support all engines
```

Accepted values are `"claude"` and `"copilot"`. Omitting the field (or
setting it to `[]`) means "all engines". The same field is available on
`[package]` for per-plugin declarations. See
[Engine and Platform Compatibility](engine-platform-compatibility.md).

## Re-running on an existing repository

`aipm init` is **idempotent**. You can safely run it again after a partial setup,
after cloning a repository, or after manually adding a missing piece:

```bash
# Add a workspace manifest to a repo that only has a marketplace
aipm init --workspace

# Retrofit Claude Code support to a Copilot-only workspace
aipm init --marketplace --engine claude

# Re-run with all options — aipm skips what already exists
aipm init --yes --workspace --marketplace --engine claude,copilot
```

For each artifact, the command prints what happened:

```
Using existing aipm.toml in .
Using existing .ai/ marketplace in .
Found existing Claude Code marketplace manifest at .ai/.claude-plugin/marketplace.json
Configured Copilot CLI settings
```

When every requested artifact was already present, a warning is emitted and the
command exits successfully without creating any files.

## Non-interactive usage

When `--yes` is passed (or when stdin is not a TTY), all interactive prompts are
skipped and defaults are used. The default engine when no `--engine` flag is
provided is **copilot**. Combine with explicit flags for fully automated
initialization in CI or scripts:

```bash
# Automated full setup — Copilot engine, CI environment
aipm init --yes --workspace --marketplace --name my-org-plugins

# Automated full setup — Claude Code engine
aipm init --yes --engine claude --workspace --marketplace --name my-org-plugins

# Both engines
aipm init --yes --engine claude,copilot --workspace --marketplace
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

When `--engine claude` (or `--engine claude,copilot`) is used, `aipm init` creates or
merges `.claude/settings.json`:

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

### Copilot CLI

When `--engine copilot` (or `--engine claude,copilot`) is used, `aipm init` creates
`.github/copilot-instructions.md` with a managed marketplace-pointer block. GitHub
Copilot CLI reads this file for project-level instructions, making the `.ai/` marketplace
discoverable. An existing `copilot-instructions.md` is never modified.

## Error conditions

| Error message | Cause | Fix |
|---|---|---|
| `existing manifest at <path> is invalid: …` | `aipm.toml` exists but contains a TOML syntax error or missing required fields | Fix or remove the malformed `aipm.toml` before re-running |
| `existing marketplace manifest at <path> is invalid: …` | A marketplace manifest (e.g. `.ai/.claude-plugin/marketplace.json`) exists but cannot be parsed as JSON | Fix or remove the malformed manifest before re-running |

> **No action needed when artifacts already exist.** If every artifact you
> asked for (`--workspace`, `--marketplace`) was already present, `aipm init`
> succeeds silently (a warning is emitted) and exits without creating any new
> files. Use `--workspace` or `--marketplace` flags to target only the missing
> piece, or inspect the existing files with `aipm lint`.

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
