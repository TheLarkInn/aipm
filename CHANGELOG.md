# Changelog

All notable changes to this project will be documented in this file.

## [0.22.4] - 2026-04-24

### Features

- **NuGet publishing pipeline** — `aipm` is now published to [nuget.org](https://www.nuget.org/packages/aipm) as a multi-RID native package (`win-x64`, `linux-x64`, `osx-x64`, `osx-arm64`), enabling Azure DevOps pipelines to install via `dotnet restore` without `curl | sh`. See [docs/guides/install-nuget.md](docs/guides/install-nuget.md).
- **Azure DevOps lint reporter enrichment** — `ci-azure` reporter now emits richer `##vso[task.logissue]` lines with `help_text` and `help_url` fields, collapsible per-file `##[group]` sections, and a `SucceededWithIssues` completion signal on warnings-only runs.

### Documentation

- Add `docs/guides/install-nuget.md` — Azure DevOps NuGet installation guide with caching and lint integration.
- Add `aipm make plugin` guide and update command table in README.
- Add `aipm update` guide and lockfile semantics reference.
- Add `aipm init` workspace initialization guide.
- Add VS Code extension guide and `aipm lsp` command reference.
- Add `instructions/oversized` rule documentation and `18-rule` lint coverage notes.

### `aipm make plugin` (v0.22.0+)

- **`aipm make plugin`** — new scaffolding command that creates plugin directories inside an existing `.ai/` marketplace, writes `.claude-plugin/plugin.json`, and registers the plugin in `marketplace.json`. Supports `--engine claude|copilot|both|lsp|extension` and `--yes` for non-interactive use.

### `instructions/oversized` lint rule (v0.20.0+)

- New rule `instructions/oversized` — warns when instruction files (`CLAUDE.md`, `AGENTS.md`, `COPILOT.md`, `GEMINI.md`, `INSTRUCTIONS.md`, `*.instructions.md`) exceed the configured line or character limit. Configurable via `resolve-imports`, `lines`, and `characters` options in `aipm.toml`.

## [0.19.7] - 2026-04-11

### Features

- **`aipm` consumer CLI** — `init`, `install`, `update`, `uninstall`, `link`, `unlink`, `list`, `lint`, `migrate`, `lsp` commands
- **`aipm-pack` author CLI** — `init` command for scaffolding new plugin packages
- **`aipm lint`** — unified, gitignore-aware quality linter with 17 rules across `skill/`, `agent/`, `hook/`, `plugin/`, `marketplace/`, and `source/` categories; supports `human`, `json`, `ci-github`, and `ci-azure` reporters
- **`aipm migrate`** — recursive discovery and migration of Claude Code (`.claude/`) and Copilot CLI (`.github/`) configurations into structured `.ai/` marketplace plugins; supports dry-run, destructive cleanup, and all artifact types (skills, agents, MCP servers, hooks, commands, output styles, extensions, LSP servers)
- **`aipm lsp`** — Language Server Protocol server powering real-time lint diagnostics, `aipm.toml` completions, and hover documentation
- **`vscode-aipm` extension** — VS Code integration via LSP; inline diagnostics, rule-ID completions, hover docs, and TOML schema validation for `aipm.toml`
- **Multi-source install** — install plugins from registry, `github:`, `git:`, `local:`, and `market:`/`marketplace:` spec formats
- **Global plugin registry** — `~/.aipm/` store with engine scoping and name-conflict detection
- **Download cache** — 5 cache policies with per-entry TTL
- **Source security** — configurable allowlist with path-traversal protection
- **Workspace support** — `[workspace]` manifest with member glob expansion and shared lints config
- **Engine & platform compatibility** — two-tier validation against `aipm.toml` `engines` field and marker files
- **`aipm.toml` JSON Schema** — available at `schemas/aipm.toml.schema.json` and via SchemaStore

