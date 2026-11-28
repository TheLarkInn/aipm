---
date: 2026-04-01 22:18:27 CDT
researcher: Claude
git_commit: baedb29fad7abcf5627bfa0cd345728e3beb3b1a
branch: main
repository: aipm
topic: "Port plugin validation rules into aipm lint"
tags: [research, lint, validation, plugin-structure, vscode-settings]
status: complete
last_updated: 2026-04-01
last_updated_by: Claude
---

# Research: Porting Plugin Validation Rules to aipm Lint

## Research Question

Investigate plugin validation patterns used in large enterprise monorepos and the existing aipm lint architecture to plan adding plugin-level structural lint rules to aipm.

## Summary

Large repos that adopt the `.ai/` marketplace structure commonly enforce plugin-level structural validation via build-time scripts and test suites. Two common validation functions are: (1) auditing `plugin.json` files for existence and consistency with the filesystem, and (2) validating that `.vscode/settings.json` location entries correctly register plugin skill/agent directories. We are porting 6 of these checks into aipm's existing `Rule` trait-based lint system.

aipm's lint system is well-architected for extension: zero-sized structs implementing a 4-method `Rule` trait, shared scanning infrastructure in `scan.rs`, `MockFs`-based testing via `test_helpers.rs`, and config-driven severity overrides per rule.

## Rules to Port

### Tier 1: Plugin Structure

| # | Proposed Rule ID | Severity | What It Checks |
|---|---|---|---|
| 1 | `plugin/missing-manifest` | **Error** (on by default) | Plugin dir exists but no `.claude-plugin/plugin.json` |
| 2 | `plugin/banned-mcp-servers` | **Error** (on by default) | plugin.json contains `"mcpServers"` key (breaks Claude Code) |
| 3 | `repo/banned-mcp-config` | Warning | MCP config files at repo root instead of in plugins |

### Tier 2: VS Code Location Sync

| # | Proposed Rule ID | Severity | What It Checks |
|---|---|---|---|
| 4 | `vscode/missing-skills-location` | Warning | Plugin with `skills/` not in `chat.agentSkillsLocations` |
| 5 | `vscode/missing-agents-location` | Warning | Plugin with `agents/` not in `chat.agentFilesLocations` |
| 6 | `vscode/ineffective-plugin-locations` | Warning | `chat.pluginLocations` present and non-empty in repo settings (user-level only, no effect at repo level) |

## Detailed Findings

### Existing aipm Lint Architecture

#### Rule Trait (`crates/libaipm/src/lint/rule.rs:16-31`)
- 4 methods: `id()`, `name()`, `default_severity()`, `check(source_dir, fs)`
- `check` receives `&Path` (source dir like `.ai/`) and `&dyn Fs`
- Returns `Result<Vec<Diagnostic>, Error>`
- Rules are `Send + Sync`, stateless zero-sized structs

#### Rule Registration (`crates/libaipm/src/lint/rules/mod.rs`)
- `for_marketplace()` returns `Vec<Box<dyn Rule>>` for `.ai/` source type (currently 11 rules)
- `for_claude()` and `for_copilot()` for other source types
- `for_source()` dispatches by source string
- New rules: add `pub mod` + `Box::new()` entry in the factory

#### Scanning (`crates/libaipm/src/lint/rules/scan.rs`)
- `scan_skills()` → `Vec<FoundSkill>` (path, frontmatter, content)
- `scan_agents()` → `Vec<FoundAgent>` (path, frontmatter)
- `scan_hook_files()` → `Vec<(PathBuf, String)>`
- Pattern: iterate `.ai/<plugin>/<subdir>/`, skip errors with `let Ok(...) = ... else { continue; }`

#### Test Helpers (`crates/libaipm/src/lint/rules/test_helpers.rs`)
- `MockFs` with `exists`, `dirs`, `files`, `written` fields
- `add_skill(plugin, skill, content)`, `add_agent(plugin, name, content)`, `add_hooks(plugin, content)`
- `add_existing(path)` for `fs.exists()` checks

#### Pipeline (`crates/libaipm/src/lint/mod.rs:42`)
- Auto-discovers source types, gets rules via `for_source()`, runs each rule, applies config overrides, filters by ignore globs
- `Options.dir` is the workspace root; `scan_dir = options.dir.join(source)` becomes e.g. `.ai/`

### Common Enterprise Validation Patterns

#### Plugin.json Audit Pattern
- Iterate ALL marketplace plugins (not just enabled ones)
- For each: check dir exists, check `.claude-plugin/plugin.json` exists
- Check filesystem directories (skills/, agents/) are consistent with plugin.json declarations
- Check for banned keys like `"mcpServers"` which break Claude Code

#### VS Code Settings Location Validation Pattern
- Read `.vscode/settings.json` as JSON
- Extract `chat.agentSkillsLocations` and `chat.agentFilesLocations` (both `Record<string, boolean>`)
- For each marketplace plugin with skills/agents dirs, check the corresponding path is `true` in settings
- Expected path format: `.ai/{pluginDir}/skills` (no leading `./`)

#### Banned MCP Config Pattern
- Check that `.vscode/mcp.json`, `.copilot/mcp.json`, `mcp.json`, `mcpServers.json` do NOT exist at repo root
- Rationale: all MCP servers must be declared in per-plugin `.mcp.json` files

#### Ineffective pluginLocations Pattern
- `chat.pluginLocations` only works at VS Code user level, not repo level
- Warn if present and non-empty in `.vscode/settings.json`

### Key Types

#### plugin.json (subset relevant to linting)
```json
{
  "name": "my-plugin",
  "version": "0.1.0",
  "description": "...",
  "skills": true,    // optional — declares plugin has skills
  "agents": true,    // optional — declares plugin has agents
  "mcp": true        // optional — declares plugin has MCP config
}
```

#### marketplace.json
```json
{
  "name": "marketplace-name",
  "plugins": [
    { "name": "my-plugin", "source": "./my-plugin", "description": "..." }
  ]
}
```

### aipm's Current Plugin/Marketplace Model

- `marketplace.json` at `.ai/.claude-plugin/marketplace.json` — array of `{name, source, description}` entries
- `plugin.json` at `.ai/<plugin>/.claude-plugin/plugin.json` — `{name, version, description}` (currently no `skills`/`agents`/`mcp` fields)
- No typed Rust structs for either — constructed/read as `serde_json::Value`
- `serde_json` is already a dependency of libaipm (used by `hook_unknown_event.rs`)
- **No VS Code settings awareness** — aipm has zero knowledge of `.vscode/settings.json`

## Architecture Documentation

### New Scanning Infrastructure Needed

A new `scan_plugin_dirs()` function in `scan.rs` to enumerate plugin directories and their metadata:

```rust
pub struct FoundPluginDir {
    pub name: String,
    pub path: PathBuf,
    pub plugin_json: Option<serde_json::Value>,
    pub plugin_json_raw: Option<String>,
    pub has_skills_dir: bool,
    pub has_agents_dir: bool,
}
```

### Workspace Root Access

Rules receive `source_dir` (e.g., `/workspace/.ai/`). For VS Code settings and repo-root checks, rules use `source_dir.parent()` to reach the workspace root. This avoids changing the `Rule` trait signature.

### File Layout for New Rules

| File | Rules | Structs |
|---|---|---|
| `plugin_missing_manifest.rs` | `plugin/missing-manifest` | `MissingManifest` |
| `plugin_banned_mcp_servers.rs` | `plugin/banned-mcp-servers` | `BannedMcpServers` |
| `repo_banned_mcp_config.rs` | `repo/banned-mcp-config` | `BannedMcpConfig` |
| `vscode_missing_location.rs` | `vscode/missing-skills-location`, `vscode/missing-agents-location` | `MissingSkillsLocation`, `MissingAgentsLocation` |
| `vscode_ineffective_locations.rs` | `vscode/ineffective-plugin-locations` | `IneffectivePluginLocations` |

All go in `crates/libaipm/src/lint/rules/` and register in `for_marketplace()`.

## Code References

- `crates/libaipm/src/lint/rule.rs:16-31` — Rule trait definition
- `crates/libaipm/src/lint/rules/mod.rs:36-52` — `for_marketplace()` factory
- `crates/libaipm/src/lint/rules/scan.rs:32-131` — existing scan functions
- `crates/libaipm/src/lint/rules/test_helpers.rs:19-189` — MockFs and helpers
- `crates/libaipm/src/lint/diagnostic.rs:39-53` — Diagnostic struct
- `crates/libaipm/src/lint/mod.rs:42-112` — lint pipeline
- `crates/libaipm/src/lint/rules/hook_unknown_event.rs` — example of JSON parsing in a rule
- `crates/libaipm/src/lint/rules/misplaced_features.rs` — example of `fs.exists()` checks

## Open Questions

1. aipm's `plugin.json` currently has `{name, version, description}` — no `skills`/`agents`/`mcp` fields. Undeclared component checks were deferred because the `plugin.json` schema differs between Claude Code plugins and aipm plugins (which use `aipm.toml`).
2. `chat.pluginLocations` only works at VS Code user level, so stale/orphan checks were replaced with a single "ineffective" warning.
