---
date: 2026-03-16
researcher: Claude Opus 4.6
git_commit: 99f8c8679e6d4d40c186adb2321aee66cdddb216
branch: main
repository: aipm
topic: "User story: aipm init --workspace --marketplace for workspace + .ai/ local marketplace scaffolding"
tags: [research, codebase, init, workspace, marketplace, claude-code, copilot, user-story]
status: complete
last_updated: 2026-03-16
last_updated_by: Claude Opus 4.6
last_updated_note: "Redesigned marketplace folder from claude-plugins/ to .ai/, added Copilot settings integration"
---

# Research: `aipm init --workspace --marketplace`

## Research Question

Add a new user story for the `aipm` binary (consumer CLI) which installs packages. The user should be able to run `aipm init --workspace --marketplace`, which should generate a workspace manifest with friendly default configuration AND generate a default local marketplace directory (`.ai/`) with the necessary Claude Code and Copilot settings to point to it.

## Summary

This feature adds a new `init` subcommand to the **`aipm` consumer binary** (not `aipm-pack`). The new `aipm init --workspace --marketplace` command targets **plugin consumers** who want to set up their repository for AI plugin management. It generates:

1. **A workspace `aipm.toml`** at the repo root with `[workspace]` section, sensible defaults, and a `[dependencies]` section ready for registry packages.
2. **A `.ai/` local marketplace directory** — a tool-agnostic convention for housing AI plugins — pre-populated with a starter plugin scaffold (skills, agents, hooks, MCP servers).
3. **Claude Code settings** (`.claude/settings.json`) configured to discover plugins from `.ai/` as a local marketplace.
4. **Copilot settings** (`.github/agents/` directory and VS Code `chat.agentFilesLocations`) configured to discover agents from `.ai/`.

The `.ai/` directory is intentionally **tool-agnostic** — it's not `.claude/` or `.github/agents/` but a neutral convention that AIPM owns and both tools can point to.

## Detailed Findings

### 1. Current State: What Exists Today

#### 1.1 `aipm` Binary (Consumer CLI)
- **Location**: `crates/aipm/src/main.rs:1-6`
- **Current capability**: Only prints version (`aipm 0.1.0`). No subcommands.
- **No `init` command exists** on the consumer binary.

#### 1.2 `aipm-pack init` (Author CLI)
- **Location**: `crates/aipm-pack/src/main.rs:20-35` (CLI definition), `crates/libaipm/src/init.rs:57-97` (implementation)
- **Generates**: `[package]` manifest only — name, version, type, edition
- **Does NOT generate**: `[workspace]` section, `plugins_dir`, `members` globs, marketplace structure
- **Flags**: `--name`, `--type`, positional `dir`

#### 1.3 Manifest Schema Support for Workspaces
- **Location**: `crates/libaipm/src/manifest/types.rs:68-78`
- **`Workspace` struct**: Already supports `members: Vec<String>`, `plugins_dir: Option<String>`, `dependencies: Option<BTreeMap<String, DependencySpec>>`
- **Validation**: `validate.rs:90-94` validates `[workspace.dependencies]` but has no workspace-specific validation
- **Parsing**: `mod.rs:21-49` — full round-trip parse/validate works for workspace manifests (proven by test `parse_workspace_root_manifest` at `mod.rs:105-134`)

#### 1.4 Feature List Entry
- **Location**: `research/feature-list.json`, Feature 4 (P0: aipm-pack init)
- **Step 6**: "Support init at repo root: generate [workspace] section with members = ['claude-plugins/*']" — documented but **not implemented**
- **Step 7**: "Support init inside plugins_dir: generate [package] section only (member manifest)" — documented but **not implemented**

#### 1.5 BDD Scenarios
- **`tests/features/manifest/init.feature`**: 7 scenarios, all for `aipm-pack init` (plugin authoring). No workspace init scenarios.
- **`tests/features/monorepo/orchestration.feature`**: Has workspace-related scenarios but no `aipm init` scenarios.

### 2. AI Tool Configuration Models

#### 2.1 Claude Code Plugin Discovery
Claude Code discovers plugins by scanning directories. A plugin is any directory containing:
- `.claude-plugin/plugin.json` — optional manifest
- `skills/<name>/SKILL.md` — skill definitions
- `agents/*.md` — subagent definitions
- `hooks/hooks.json` — hook configurations
- `.mcp.json` — MCP server definitions
- `.lsp.json` — LSP server configurations
- `settings.json` — default settings

**Local marketplace registration** (`.claude/settings.json`):
```json
{
  "extraKnownMarketplaces": {
    "local": {
      "source": {
        "source": "local",
        "path": ".ai"
      }
    }
  }
}
```

Source: [Claude Code Discover Plugins](https://code.claude.com/docs/en/discover-plugins)

#### 2.2 GitHub Copilot Agent/Plugin Discovery
Copilot discovers agents from `.agent.md` files in well-known directories:
- `.github/agents/` in the workspace root (primary)
- `~/.copilot/agents/` in the user home directory
- Additional paths via `chat.agentFilesLocations` VS Code setting

**Plugin marketplace registration** (VS Code `settings.json`):
```json
{
  "chat.agentFilesLocations": [".ai"],
  "chat.plugins.marketplaces": ["file:///.ai"]
}
```

**MCP servers** (`.copilot/mcp-config.json` for CLI):
```json
{
  "mcpServers": {}
}
```

Sources:
- [Custom agents in VS Code](https://code.visualstudio.com/docs/copilot/customization/custom-agents)
- [Creating custom agents for Copilot CLI](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/create-custom-agents-for-cli)

### 3. Proposed `aipm init` Command Design

#### 3.1 Command: `aipm init --workspace --marketplace`

The `aipm` binary gets an `init` subcommand with two independent flags that compose:

| Flag | Effect |
|------|--------|
| `--workspace` | Generate `aipm.toml` with `[workspace]` section at repo root |
| `--marketplace` | Generate `.ai/` local marketplace directory + tool settings |
| Both together | Full workspace + marketplace scaffolding (recommended for new projects) |
| Neither (default) | Same as both together — zero-friction onboarding |

#### 3.2 Generated Workspace Manifest (`aipm.toml`)

```toml
# AI Plugin Manager — Workspace Configuration
# Docs: https://github.com/microsoft/aipm

[workspace]
members = [".ai/*"]
plugins_dir = ".ai"

# Shared dependency versions for all workspace members.
# Members reference these via: dep = { workspace = "^" }
# [workspace.dependencies]

# Direct registry installs (available project-wide).
# [dependencies]

# Environment requirements for all plugins in this workspace.
# [environment]
# requires = ["git"]
```

Key design decisions:
- `members = [".ai/*"]` — workspace members are plugins inside the `.ai/` marketplace directory
- `plugins_dir = ".ai"` — explicitly names the marketplace directory
- Commented-out sections show what's available without cluttering the default
- No `[package]` section — this is a virtual workspace (Cargo model)

#### 3.3 Generated `.ai/` Marketplace Directory Structure

When `--marketplace` is passed, create the `.ai/` directory with a starter local plugin and tool integration files:

```
.ai/
├── .gitignore                     # Managed by aipm (registry installs go here)
├── starter/                       # A starter local plugin (git-tracked)
│   ├── .claude-plugin/
│   │   └── plugin.json           # Claude Code plugin manifest
│   ├── aipm.toml                 # AIPM member manifest
│   ├── skills/
│   │   └── hello/
│   │       └── SKILL.md          # Starter skill template
│   ├── agents/
│   │   └── .gitkeep
│   ├── hooks/
│   │   └── .gitkeep
│   └── .mcp.json                 # Empty MCP config stub
```

**`.ai/starter/.claude-plugin/plugin.json`:**
```json
{
  "name": "starter",
  "version": "0.1.0",
  "description": "Starter plugin — customize or rename this directory"
}
```

**`.ai/starter/aipm.toml`:**
```toml
[package]
name = "starter"
version = "0.1.0"
type = "composite"
edition = "2024"
description = "Starter plugin — customize or rename this directory"

# [dependencies]
# Add registry dependencies here, e.g.:
# shared-skill = "^1.0"

[components]
skills = ["skills/hello/SKILL.md"]
```

**`.ai/starter/skills/hello/SKILL.md`:**
```markdown
---
description: A starter skill — describe what it does so Claude knows when to use it
---

# Hello Skill

This is a starter skill template. Customize the description in the frontmatter
above so your AI coding tool can auto-discover when to invoke this skill.

## Instructions

Replace this content with instructions for the AI agent when this skill is active.
```

**`.ai/starter/.mcp.json`:**
```json
{
  "mcpServers": {}
}
```

**`.ai/.gitignore`:**
```
# Managed by aipm — registry-installed plugins are symlinked here.
# Do not edit the section between the markers.
# === aipm managed start ===
# === aipm managed end ===
```

#### 3.4 Generated Tool Settings Files

When `--marketplace` is passed, generate settings files for both Claude Code and Copilot so they discover `.ai/` as a local marketplace. Files are only generated if they don't already exist.

##### Claude Code: `.claude/settings.json`

```json
{
  "permissions": {},
  "enabledPlugins": [],
  "extraKnownMarketplaces": {
    "local": {
      "source": {
        "source": "local",
        "path": ".ai"
      }
    }
  }
}
```

This registers `.ai/` as a local marketplace so Claude Code discovers plugins inside it. Each subdirectory of `.ai/` that contains a `.claude-plugin/plugin.json` or standard component directories becomes a discoverable plugin.

##### Copilot: `.vscode/settings.json`

If a `.vscode/settings.json` already exists, the command merges the `chat.agentFilesLocations` key. If it doesn't exist, it creates:

```json
{
  "chat.agentFilesLocations": [".ai"]
}
```

This tells VS Code Copilot to scan `.ai/` for `.agent.md` files in addition to its default locations (`.github/agents/`).

##### Copilot CLI: `.copilot/mcp-config.json`

If the file doesn't already exist:

```json
{
  "mcpServers": {}
}
```

This is a stub for Copilot CLI MCP server configuration, ready for the user to add servers.

#### 3.5 Complete Generated Tree (both flags)

```
repo-root/
├── aipm.toml                          # Workspace manifest ([workspace] section)
├── .ai/                               # Local marketplace (tool-agnostic)
│   ├── .gitignore                     # Managed by aipm
│   └── starter/                       # Starter plugin
│       ├── .claude-plugin/
│       │   └── plugin.json
│       ├── aipm.toml
│       ├── skills/
│       │   └── hello/
│       │       └── SKILL.md
│       ├── agents/
│       │   └── .gitkeep
│       ├── hooks/
│       │   └── .gitkeep
│       └── .mcp.json
├── .claude/
│   └── settings.json                  # Claude Code → points to .ai/
├── .vscode/
│   └── settings.json                  # Copilot → points to .ai/
└── .copilot/
    └── mcp-config.json                # Copilot CLI MCP stub
```

### 4. Architecture: Where This Lives in the Codebase

#### 4.1 New Module: `crates/libaipm/src/workspace_init.rs`

Separate from `init.rs` (which handles plugin-level init for `aipm-pack`). The workspace init module handles:
- Workspace manifest generation (`generate_workspace_manifest`)
- Marketplace directory scaffolding (`generate_marketplace`)
- Claude Code settings generation (`generate_claude_settings`)
- Copilot settings generation (`generate_copilot_settings`)
- Idempotency checks (don't overwrite existing files)

#### 4.2 New CLI Subcommand: `crates/aipm/src/main.rs`

Add clap `Parser` + `Subcommand` to the `aipm` binary (currently has no CLI framework):

```rust
#[derive(Parser)]
#[command(name = "aipm", version = libaipm::version(), about = "AI Plugin Manager — consumer CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a workspace for AI plugin management.
    Init {
        /// Generate a workspace manifest (aipm.toml with [workspace] section).
        #[arg(long)]
        workspace: bool,

        /// Generate a .ai/ local marketplace with tool settings.
        #[arg(long)]
        marketplace: bool,

        /// Directory to initialize (defaults to current directory).
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
}
```

#### 4.3 Shared Library Updates: `crates/libaipm/src/lib.rs`

Add `pub mod workspace_init;` to expose the new module.

### 5. Behavioral Rules

| Rule | Behavior |
|------|----------|
| **Idempotent** | Never overwrite existing `aipm.toml`, `.ai/`, `.claude/settings.json`, or `.vscode/settings.json`. Error with "already initialized" if workspace manifest exists. |
| **Composable flags** | `--workspace` and `--marketplace` can be used independently or together. Neither requires the other. |
| **Merge, don't clobber** | If `.vscode/settings.json` exists, merge `chat.agentFilesLocations` key rather than overwriting the file. Same for `.claude/settings.json` with `extraKnownMarketplaces`. |
| **Name inference** | Workspace name is not required (virtual workspace has no package name). Starter plugin defaults to "starter". |
| **No network** | `init` is fully offline — no registry calls, no marketplace fetches. |
| **Git-friendly** | Generated `.gitignore` in `.ai/` has markers for aipm-managed entries. |
| **Cross-platform** | Uses `create_dir_all`, forward slashes in TOML/JSON, no platform-specific paths. |
| **Tool-agnostic directory** | `.ai/` is not tied to any specific AI tool — it's the AIPM convention that tools point to. |

### 6. Distinction from `aipm-pack init`

| Aspect | `aipm init` (consumer) | `aipm-pack init` (author) |
|--------|----------------------|--------------------------|
| **Binary** | `aipm` | `aipm-pack` |
| **Purpose** | Set up a repo for plugin consumption + local development | Scaffold a single plugin package for publishing |
| **Manifest** | `[workspace]` section | `[package]` section |
| **Directory** | Repo root + `.ai/` marketplace | Single plugin directory |
| **Tool settings** | Generates `.claude/settings.json`, `.vscode/settings.json`, `.copilot/mcp-config.json` | None |
| **Flags** | `--workspace`, `--marketplace` | `--name`, `--type` |
| **Target user** | Developer setting up their project | Plugin author creating a publishable package |

### 7. Why `.ai/` Instead of `claude-plugins/`

| Consideration | `claude-plugins/` (old) | `.ai/` (new) |
|---------------|------------------------|--------------|
| **Tool neutrality** | Implies Claude Code only | Works for any AI tool |
| **Convention** | Claude Code's default, but not universal | New AIPM-owned convention |
| **Dotfile** | Visible in file explorer | Hidden by convention (dotfile), less visual clutter |
| **Discoverability** | Both tools need configuration | Both tools need configuration |
| **Existing ecosystem** | Claude Code already uses `claude-plugins/` for some projects | Fresh start, no legacy expectations |
| **Name length** | 15 characters | 3 characters |

The trade-off: `.ai/` requires explicit settings in both Claude Code and Copilot (neither auto-discovers it). But since `aipm init --marketplace` generates those settings files, the user gets zero-config setup regardless.

## Code References

- `crates/aipm/src/main.rs:1-6` — Current `aipm` binary (version-only, no subcommands)
- `crates/aipm-pack/src/main.rs:12-35` — Existing CLI pattern to follow for `aipm`
- `crates/libaipm/src/init.rs:57-97` — Existing init implementation pattern
- `crates/libaipm/src/init.rs:186-203` — Manifest generation pattern (string template)
- `crates/libaipm/src/manifest/types.rs:68-78` — `Workspace` struct (already exists)
- `crates/libaipm/src/manifest/mod.rs:105-134` — Workspace manifest parse test (proves round-trip works)
- `crates/libaipm/src/manifest/validate.rs:76-111` — Validation pipeline pattern
- `crates/libaipm/src/manifest/error.rs:10-75` — Error modeling pattern

## Architecture Documentation

### Existing Patterns to Follow

1. **Module structure**: New functionality goes in `libaipm` as a module (`workspace_init.rs`), exposed via `lib.rs`, consumed by the binary crate
2. **Error handling**: `thiserror::Error` enum with typed variants, no `.unwrap()` or `panic!`
3. **Output**: `std::io::Write` with `writeln!`, never `println!`
4. **CLI**: `clap::Parser` + `Subcommand`, `run() -> Result<>` pattern, `main() -> ExitCode`
5. **Testing**: Unit tests in-module, E2E tests in `tests/` using `assert_cmd` + `tempfile` + `predicates`
6. **Lint compliance**: All code must pass `cargo clippy --workspace -- -D warnings` with zero violations

### New BDD Scenarios Needed

A new feature file `tests/features/manifest/workspace-init.feature` should cover:

1. Initialize a workspace in an empty directory
2. Initialize a workspace with `--marketplace` generates `.ai/` marketplace
3. Reject initialization if `aipm.toml` already exists
4. `--workspace` flag generates `[workspace]` section with members `.ai/*`
5. `--marketplace` flag generates `.ai/` with starter plugin
6. Starter plugin has valid `aipm.toml` (round-trip parse test)
7. Starter plugin has Claude Code plugin structure (`.claude-plugin/plugin.json`)
8. Generated `.gitignore` has aipm managed markers
9. `.claude/settings.json` is created pointing to `.ai/` marketplace
10. `.vscode/settings.json` is created with `chat.agentFilesLocations`
11. `.copilot/mcp-config.json` stub is created
12. Existing settings files are not overwritten (merge behavior)
13. Both flags together produce complete scaffolding
14. `--marketplace` without `--workspace` generates only marketplace + settings
15. Default init with no flags creates both workspace and marketplace

## Historical Context (from research/)

- `research/docs/2026-03-09-aipm-cucumber-feature-spec.md` — BDD feature spec resolved "No marketplace interop" — AIPM defines its own registry model rather than integrating with Claude Code's marketplace format. This feature creates a **local** marketplace structure, not a registry marketplace.
- `research/docs/2026-03-09-cargo-core-principles.md` — Cargo workspace model (virtual workspace = `[workspace]` without `[package]`) is the inspiration for the workspace manifest design.
- `research/docs/2026-03-09-pnpm-core-principles.md` — pnpm workspace protocol (`workspace:^`) and catalogs inform the workspace dependency model.
- `research/feature-list.json` — Feature 4 steps 6-7 document workspace-level init as planned but not implemented.
- `research/progress.txt` — Records Feature 4 as complete, but workspace-level init was scoped out (only plugin-level init was implemented).

## Related Research

- `research/docs/2026-03-16-claude-code-defaults.md` — Claude Code plugin discovery model, marketplace structure, and default configuration
- `research/docs/2026-03-16-copilot-agent-discovery.md` — Copilot agent/plugin discovery, `.github/agents/`, `chat.agentFilesLocations`, marketplace registration

## Open Questions

1. **Should `aipm init` (no flags) do anything?** Options: (a) error with "specify --workspace and/or --marketplace", (b) default to `--workspace --marketplace`, (c) interactive prompt. Recommendation: default to `--workspace --marketplace` for zero-friction onboarding.

2. **Starter plugin naming**: Should the starter plugin be named "starter", or inferred from the repo directory name? Recommendation: use "starter" as a universal default — the user will rename it.

3. **Should the workspace manifest include a commented `[catalog]` section?** Catalogs are P1 but showing the section educates users about the capability. Recommendation: include as a comment.

4. **Merge strategy for existing `.vscode/settings.json`**: If the file already has `chat.agentFilesLocations`, should we append `.ai` or skip? Recommendation: append if `.ai` is not already in the array, skip if it is.

5. **Should `.ai/` also contain a top-level `README.md`?** This would explain the directory's purpose to developers who encounter it. Recommendation: yes, a short README explaining that this is an AIPM-managed marketplace directory.
