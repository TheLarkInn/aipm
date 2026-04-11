# Changelog

All notable changes to this project will be documented in this file.

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
