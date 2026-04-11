# Documentation

Reference documentation for `aipm` and `aipm-pack`.

## Guides

How-to guides for common tasks:

| Guide | Description |
|-------|-------------|
| [Creating a Plugin](guides/creating-a-plugin.md) | Scaffold a new plugin package with `aipm-pack init` |
| [Installing from Git](guides/install-git-plugin.md) | Install plugins from GitHub or any git repository |
| [Installing from Local Paths](guides/install-local-plugin.md) | Install plugins from your local filesystem |
| [Installing from Marketplaces](guides/install-marketplace-plugin.md) | Install plugins from curated marketplace repos |
| [Global Plugin Installation](guides/global-plugins.md) | Install plugins globally across all projects |
| [Local Development](guides/local-development.md) | Develop plugins locally with `aipm link` / `aipm unlink` |
| [Migrating Existing Configurations](guides/migrate.md) | Convert `.claude/` / `.github/` configs to marketplace plugins |
| [Migrating — Step-by-step](guides/migrating-existing-configs.md) | Dry-run, destructive cleanup, recursive discovery walkthrough |
| [Configuring Lint](guides/configuring-lint.md) | Tune rule severity, suppress noise, exclude directories |
| [Using `aipm lint`](guides/lint.md) | CLI flags, output formats, CI integration, rules reference |
| [Verbosity & Logging](guides/verbosity-and-logging.md) | Verbosity flags, `AIPM_LOG`, log file, CI recommendations |
| [Engine & Platform Compatibility](guides/engine-platform-compatibility.md) | Declare supported AI tools and operating systems |
| [Download Cache](guides/cache-management.md) | Cache policies, TTL, and garbage collection |
| [Source Security](guides/source-security.md) | Source allowlists and path traversal protection |
| [VS Code Extension](guides/vscode-extension.md) | Real-time lint diagnostics, completions, and hover in VS Code via `aipm lsp` |

## Lint Rule Reference

Quality rules enforced by `aipm lint`:

### `skill/`

| Rule | Default | Description |
|------|---------|-------------|
| [missing-name](rules/skill/missing-name.md) | warn | Skill has no `name` in frontmatter |
| [missing-description](rules/skill/missing-description.md) | warn | Skill has no `description` in frontmatter |
| [name-invalid-chars](rules/skill/name-invalid-chars.md) | warn | Skill name contains disallowed characters |
| [name-too-long](rules/skill/name-too-long.md) | warn | Skill name exceeds the maximum length |
| [description-too-long](rules/skill/description-too-long.md) | warn | Skill description exceeds the maximum length |
| [invalid-shell](rules/skill/invalid-shell.md) | error | Skill references an unrecognized shell |
| [oversized](rules/skill/oversized.md) | warn | Skill file exceeds the recommended size |

### `hook/`

| Rule | Default | Description |
|------|---------|-------------|
| [unknown-event](rules/hook/unknown-event.md) | error | Hook references an unrecognized event name |
| [legacy-event-name](rules/hook/legacy-event-name.md) | warn | Hook uses a deprecated event name |

### `agent/`

| Rule | Default | Description |
|------|---------|-------------|
| [missing-tools](rules/agent/missing-tools.md) | warn | Agent has no declared tools |

### `plugin/`

| Rule | Default | Description |
|------|---------|-------------|
| [broken-paths](rules/plugin/broken-paths.md) | error | Plugin manifest references a file that does not exist |
| [missing-manifest](rules/plugin/missing-manifest.md) | error | Plugin directory is missing `.claude-plugin/plugin.json` |
| [missing-registration](rules/plugin/missing-registration.md) | error | Plugin directory is not listed in `marketplace.json` |
| [required-fields](rules/plugin/required-fields.md) | error | `plugin.json` is missing one or more required fields |

### `marketplace/`

| Rule | Default | Description |
|------|---------|-------------|
| [plugin-field-mismatch](rules/marketplace/plugin-field-mismatch.md) | error | `marketplace.json` entry `name`/`description` differs from `plugin.json` |
| [source-resolve](rules/marketplace/source-resolve.md) | error | `marketplace.json` entry `source` path does not exist on disk |

### `source/`

| Rule | Default | Description |
|------|---------|-------------|
| [misplaced-features](rules/source/misplaced-features.md) | warn | Feature file is outside the `.ai/` marketplace directory |

## See also

- [README](../README.md) — full command reference for `aipm` and `aipm-pack`
- [Manifest format](../README.md#manifest-format-aipmtoml) — `aipm.toml` schema
- [Workspace lints](../README.md#workspace-root-manifest) — `[workspace.lints]` configuration
