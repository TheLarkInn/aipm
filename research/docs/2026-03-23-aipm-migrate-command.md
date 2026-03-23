---
date: 2026-03-23 09:44:05 PDT
researcher: Claude
git_commit: 816e338a6ec58e13328a1703f554170b0b59a27d
branch: main
repository: aipm
topic: "aipm migrate command — scan .claude/ skills and migrate to AI plugin marketplace"
tags: [research, codebase, migrate, skills, scanner, detector, plugin, marketplace]
status: complete
last_updated: 2026-03-23
last_updated_by: Claude
---

# Research: `aipm migrate` Command

## Research Question

Creating a new feature called `aipm migrate`. Day 1 goal: migrate skills inside `.claude/` folder (along with referenced scripts) into plugins in the AI Plugin Marketplace. The plugin name should match the skill name. Do not auto-enable the skill for the repo, just add it to the marketplace. Design it so that:
1. It can serve as an internal plugin system for more than just "skills" in the future (subagents, MCPs, LSPs, output styles, etc.)
2. The scanner can scan `.claude`, `.github`, `.copilot`, or other folders in the future
3. `--dry-run` and wizard capabilities exist as flags (not implemented on day 1)

## Summary

The `aipm` codebase already has a well-established pattern for CLI commands (`clap` derive with `Commands` enum in `main.rs`), filesystem abstraction (`Fs` trait), marketplace scaffolding (`.ai/` directory with `marketplace.json`), and plugin manifests (`aipm.toml`). A `migrate` command would add a new variant to the `Commands` enum, with core logic implemented in `libaipm` as a new module.

The `.claude/skills/` directory contains skill definitions as directories with `SKILL.md` entrypoints and optional supporting files (scripts, reference docs, examples). Each skill directory maps naturally to a plugin directory in `.ai/` with an `aipm.toml` manifest, `skills/` component, and optional `scripts/` component.

The extensibility goal is best achieved with a **detector trait** pattern: a `Detector` trait with a `scan()` method that returns discovered artifacts, and concrete implementations like `SkillDetector`, `AgentDetector`, etc. A `Source` enum or trait abstracts the scan target (`.claude`, `.github`, etc.).

## Detailed Findings

### 1. Existing CLI Command Pattern

**Location**: [`crates/aipm/src/main.rs`](https://github.com/TheLarkInn/aipm/blob/816e338a6ec58e13328a1703f554170b0b59a27d/crates/aipm/src/main.rs)

The `aipm` binary uses clap's derive API. The `Cli` struct has an `Option<Commands>` field. Currently, only `Commands::Init` exists. Adding `Commands::Migrate` follows the exact same pattern:

```rust
#[derive(Subcommand)]
enum Commands {
    /// Initialize a workspace ...
    Init { ... },
    /// Migrate AI tool configurations into marketplace plugins
    Migrate { ... },
}
```

The `run()` function dispatches via `match cli.command`:
- `Some(Commands::Init { ... }) => { ... }`
- `Some(Commands::Migrate { ... }) => { ... }` (new arm)
- `None => { ... }` (prints version)

**Error handling**: Returns `Result<(), Box<dyn std::error::Error>>`. Errors propagate via `?`. The `main()` function converts errors to `ExitCode::FAILURE` with a `writeln!(stderr, "error: {e}")`.

**Output**: All output uses `writeln!(stdout, ...)` with `let _ =` to discard write errors. Never `println!`.

### 2. Filesystem Abstraction

**Location**: [`crates/libaipm/src/fs.rs`](https://github.com/TheLarkInn/aipm/blob/816e338a6ec58e13328a1703f554170b0b59a27d/crates/libaipm/src/fs.rs)

The `Fs` trait provides four methods: `exists`, `create_dir_all`, `write_file`, `read_to_string`. All init logic accepts `&dyn Fs`. The migrate module should follow the same pattern for testability. May need an additional `read_dir` or `list_entries` method for scanning directories.

### 3. Marketplace Structure

**Location**: [`crates/libaipm/src/workspace_init/mod.rs`](https://github.com/TheLarkInn/aipm/blob/816e338a6ec58e13328a1703f554170b0b59a27d/crates/libaipm/src/workspace_init/mod.rs)

The `.ai/` marketplace directory contains:
- `.ai/.claude-plugin/marketplace.json` — registry with `plugins` array
- `.ai/<plugin-name>/` — each plugin directory with:
  - `aipm.toml` — plugin manifest
  - `.claude-plugin/plugin.json` — Claude Code plugin descriptor
  - Component directories (`skills/`, `agents/`, `hooks/`, `scripts/`, etc.)

**`marketplace.json` format** (generated at `workspace_init/mod.rs:435-467`):
```json
{
  "name": "local-repo-plugins",
  "owner": { "name": "local" },
  "metadata": { "description": "..." },
  "plugins": [
    {
      "name": "plugin-name",
      "version": "0.1.0",
      "description": "...",
      "source": "./plugin-name"
    }
  ]
}
```

When migrating a skill, the command needs to:
1. Create `.ai/<skill-name>/` directory with proper structure
2. Append an entry to `.ai/.claude-plugin/marketplace.json` plugins array
3. **NOT** add an `enabledPlugins` entry to `.claude/settings.json` (per requirements)

### 4. Plugin Manifest Generation

**Location**: [`crates/libaipm/src/init.rs:187-204`](https://github.com/TheLarkInn/aipm/blob/816e338a6ec58e13328a1703f554170b0b59a27d/crates/libaipm/src/init.rs)

The `generate_manifest()` function produces `aipm.toml`:
```toml
[package]
name = "<name>"
version = "0.1.0"
type = "<type>"
edition = "2024"
```

For migrated skills, the manifest should also include a `[components]` section declaring which component files exist (e.g., `skills = ["skills/<name>/SKILL.md"]`, `scripts = ["scripts/<script>"]`).

**`Components` struct** at [`manifest/types.rs:115-143`](https://github.com/TheLarkInn/aipm/blob/816e338a6ec58e13328a1703f554170b0b59a27d/crates/libaipm/src/manifest/types.rs):
```rust
pub struct Components {
    pub skills: Option<Vec<String>>,
    pub commands: Option<Vec<String>>,
    pub agents: Option<Vec<String>>,
    pub hooks: Option<Vec<String>>,
    pub mcp_servers: Option<Vec<String>>,
    pub lsp_servers: Option<Vec<String>>,
    pub scripts: Option<Vec<String>>,
    pub output_styles: Option<Vec<String>>,
    pub settings: Option<Vec<String>>,
}
```

This already covers all the component types that future detectors would need: skills, agents, hooks, MCP servers, LSP servers, scripts, output styles, settings.

### 5. PluginType Enum

**Location**: [`manifest/types.rs:207-239`](https://github.com/TheLarkInn/aipm/blob/816e338a6ec58e13328a1703f554170b0b59a27d/crates/libaipm/src/manifest/types.rs)

```rust
pub enum PluginType {
    Skill,      // "skill"
    Agent,      // "agent"
    Mcp,        // "mcp"
    Hook,       // "hook"
    Lsp,        // "lsp"
    Composite,  // "composite"
}
```

For skills migration, the plugin type would be `Skill` for single-skill plugins. If a skill references agents, hooks, or scripts, the plugin type could be `Composite`.

### 6. `.claude/` Skills Format (Source to Migrate FROM)

Skills are directories under `.claude/skills/` (or `~/.claude/skills/` for user-level):

```
.claude/
  skills/
    <skill-name>/
      SKILL.md          # Required entrypoint with YAML frontmatter
      reference.md      # Optional supporting docs
      examples/         # Optional examples
      scripts/          # Optional scripts
        my-script.ts
        my-script.py
```

**`SKILL.md` frontmatter** fields:
| Field | Description |
|-------|-------------|
| `name` | Display name / slash-command name |
| `description` | What the skill does |
| `argument-hint` | Shown in autocomplete |
| `disable-model-invocation` | `true` = only user can invoke |
| `user-invocable` | `false` = hidden from `/` menu |
| `allowed-tools` | Tools allowed without permission |
| `model` | Model override |
| `effort` | low/medium/high/max |
| `context` | `fork` for isolated subagent |
| `agent` | Which subagent to use |
| `hooks` | Lifecycle hooks scoped to skill |

**Script references in SKILL.md**: Scripts are referenced via `${CLAUDE_SKILL_DIR}/scripts/<script>` in the markdown body. This variable resolves to the skill's directory.

**Dynamic context**: Lines prefixed with `` !` `` run shell commands before content is sent to Claude.

### 7. Other `.claude/` Components (Future Detector Targets)

| Component | Location | Format |
|-----------|----------|--------|
| Subagents | `.claude/agents/*.md` | Markdown with YAML frontmatter |
| Hooks | `.claude/settings.json` → `hooks` key | JSON object with event → matcher → handler |
| MCP Servers | `.mcp.json` (root) | JSON with `mcpServers` object |
| Rules | `.claude/rules/*.md` | Markdown with optional `paths` frontmatter |
| Commands | `.claude/commands/*.md` | Legacy (maps to skills) |
| Settings | `.claude/settings.json` | JSON config |
| Memory | `.claude/projects/*/memory/` | Markdown files |

### 8. Existing Init Patterns to Follow

**Directory layout creation** ([`init.rs:131-167`](https://github.com/TheLarkInn/aipm/blob/816e338a6ec58e13328a1703f554170b0b59a27d/crates/libaipm/src/init.rs)):
- Uses `fs.create_dir_all()` for directories
- Uses `fs.write_file()` for file contents
- Creates `.gitkeep` in empty directories

**Round-trip validation**: Both `init_workspace` and `scaffold_marketplace` validate generated manifests by parsing them back through `parse_and_validate`. Migrated plugin manifests should do the same.

**Idempotency guards**: `init` checks if `aipm.toml` already exists before proceeding. `migrate` should check if a plugin with the same name already exists in `.ai/`.

### 9. Tool Adaptor Pattern

**Location**: [`crates/libaipm/src/workspace_init/adaptors/`](https://github.com/TheLarkInn/aipm/blob/816e338a6ec58e13328a1703f554170b0b59a27d/crates/libaipm/src/workspace_init/adaptors)

```rust
pub trait ToolAdaptor {
    fn name(&self) -> &'static str;
    fn apply(&self, dir: &Path, fs: &dyn Fs) -> Result<bool, Error>;
}
```

This trait is used for tool-specific settings generation (Claude Code, Copilot). For `migrate`, tool adaptors are not directly relevant but the pattern of trait-based extensibility is the model for the detector architecture.

### 10. Test Infrastructure Patterns

**E2E tests** use `assert_cmd::Command::cargo_bin("aipm")` with `tempfile::TempDir`:
- [`crates/aipm/tests/init_e2e.rs`](https://github.com/TheLarkInn/aipm/blob/816e338a6ec58e13328a1703f554170b0b59a27d/crates/aipm/tests/init_e2e.rs) — 17 tests for `aipm init`

**BDD tests** use cucumber-rs with `AipmWorld` state:
- [`crates/libaipm/tests/bdd.rs`](https://github.com/TheLarkInn/aipm/blob/816e338a6ec58e13328a1703f554170b0b59a27d/crates/libaipm/tests/bdd.rs) — 3 active feature files
- Feature files in `tests/features/manifest/`

**Unit tests** use `insta` snapshot tests for wizard prompt definitions:
- [`crates/aipm/src/wizard.rs`](https://github.com/TheLarkInn/aipm/blob/816e338a6ec58e13328a1703f554170b0b59a27d/crates/aipm/src/wizard.rs) — 13 snapshot tests
- [`crates/aipm-pack/src/wizard.rs`](https://github.com/TheLarkInn/aipm/blob/816e338a6ec58e13328a1703f554170b0b59a27d/crates/aipm-pack/src/wizard.rs) — 11 snapshot tests

**Coverage**: 89% branch coverage threshold, `wizard_tty.rs` excluded from coverage.

## Code References

### Core Files to Modify/Extend

- `crates/aipm/src/main.rs` — Add `Commands::Migrate` variant and dispatch arm
- `crates/libaipm/src/lib.rs` — Add `pub mod migrate;` export
- `crates/libaipm/src/fs.rs` — May need `read_dir` method on `Fs` trait for scanning

### Core Files to Create

- `crates/libaipm/src/migrate/mod.rs` — `migrate()` orchestrator function, `Options`, `MigrateResult`, `MigrateAction` types
- `crates/libaipm/src/migrate/detector.rs` — `Detector` trait, `DetectedArtifact` struct, `ArtifactKind` enum
- `crates/libaipm/src/migrate/skill_detector.rs` — `SkillDetector` scanning `.claude/skills/`
- `crates/libaipm/src/migrate/source.rs` — `Source` enum/struct for scan targets (`.claude`, `.github`, etc.)
- `crates/libaipm/src/migrate/emitter.rs` — `Emitter` trait/functions for writing plugin directories to `.ai/`

### Existing Files to Reference (Patterns to Follow)

- `crates/libaipm/src/init.rs` — Plugin scaffolding pattern (directory layout, manifest generation)
- `crates/libaipm/src/workspace_init/mod.rs` — Marketplace JSON manipulation, `generate_*()` functions
- `crates/libaipm/src/workspace_init/adaptors/claude.rs` — JSON merge pattern for marketplace.json updates
- `crates/libaipm/src/manifest/types.rs` — `Components` struct, `PluginType` enum
- `crates/libaipm/src/manifest/validate.rs` — Name validation (`is_valid_name`)

### Test Files to Create

- `crates/libaipm/src/migrate/mod.rs` — `#[cfg(test)]` module with unit tests
- `crates/aipm/tests/migrate_e2e.rs` — E2E tests for `aipm migrate`
- `tests/features/manifest/migrate.feature` — BDD scenarios (spec-only or active)

## Architecture Documentation

### Proposed Detector Trait Architecture

```
┌──────────────────────────────────────────────────┐
│                  aipm migrate                     │
│              (CLI dispatch layer)                 │
└──────────────┬───────────────────────────────────┘
               │
               ▼
┌──────────────────────────────────────────────────┐
│            libaipm::migrate::migrate()            │
│         (orchestrator — takes Options, Fs)        │
│                                                   │
│  1. Resolve source paths                          │
│  2. Run detectors → Vec<DetectedArtifact>         │
│  3. (Future: --dry-run returns here)              │
│  4. Emit plugins to .ai/                          │
│  5. Register in marketplace.json                  │
│  6. Return MigrateResult with actions             │
└──────┬────────────┬────────────┬─────────────────┘
       │            │            │
       ▼            ▼            ▼
┌──────────┐ ┌──────────┐ ┌──────────┐
│  Skill   │ │  Agent   │ │   MCP    │  (Future)
│ Detector │ │ Detector │ │ Detector │
└──────────┘ └──────────┘ └──────────┘
  impl Detector      impl Detector
```

### Key Types (Conceptual)

```rust
/// What kind of artifact was detected
pub enum ArtifactKind {
    Skill,
    Agent,
    Mcp,
    Hook,
    Lsp,
    OutputStyle,
    Script,    // standalone script (not part of a skill)
    Settings,
    Command,   // legacy .claude/commands/
}

/// A single detected artifact from a source folder
pub struct DetectedArtifact {
    pub kind: ArtifactKind,
    pub name: String,
    pub source_path: PathBuf,         // e.g., .claude/skills/my-skill/
    pub files: Vec<PathBuf>,          // all files belonging to this artifact
    pub referenced_scripts: Vec<PathBuf>, // scripts referenced by SKILL.md
    pub metadata: ArtifactMetadata,   // parsed frontmatter, etc.
}

/// Trait for scanning a source directory for artifacts of a specific kind
pub trait Detector {
    fn kind(&self) -> ArtifactKind;
    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<DetectedArtifact>, Error>;
}

/// A scan target
pub struct Source {
    pub name: &'static str,       // "claude", "github", "copilot"
    pub path: PathBuf,            // resolved .claude/ path
    pub detectors: Vec<Box<dyn Detector>>,
}

/// Options for the migrate command
pub struct Options<'a> {
    pub dir: &'a Path,            // project root
    pub source: Option<&'a str>,  // specific source to scan (default: all)
    pub dry_run: bool,            // (flag exists, not implemented day 1)
    pub interactive: bool,        // (flag exists, not implemented day 1)
}

/// Result of migration
pub struct MigrateResult {
    pub actions: Vec<MigrateAction>,
}

pub enum MigrateAction {
    PluginCreated { name: String, source: PathBuf },
    MarketplaceRegistered { name: String },
    Skipped { name: String, reason: String },
}
```

### Skill Migration Flow (Day 1)

1. **Scan**: `SkillDetector` reads `.claude/skills/` directory
   - For each subdirectory, check for `SKILL.md`
   - Parse frontmatter for name, description
   - Walk all files in the skill directory (recursively)
   - Identify script references in SKILL.md body (`${CLAUDE_SKILL_DIR}/scripts/...`)
2. **Emit**: For each `DetectedArtifact`:
   - Create `.ai/<skill-name>/` directory
   - Copy skill files into `.ai/<skill-name>/skills/<skill-name>/`
   - Copy referenced scripts into `.ai/<skill-name>/scripts/`
   - Generate `aipm.toml` with `type = "skill"` and `[components]` section
   - Generate `.claude-plugin/plugin.json`
3. **Register**: Append entry to `.ai/.claude-plugin/marketplace.json`
   - Do NOT modify `.claude/settings.json` enabledPlugins (per requirements)
4. **Report**: Return `MigrateResult` with all actions taken

### Guards and Edge Cases

- **No `.ai/` directory**: Error — user must run `aipm init` first (or auto-create? design decision)
- **Plugin already exists**: Skip with `MigrateAction::Skipped` reason
- **No skills found**: Report empty result (not an error)
- **Invalid skill name**: Sanitize to valid package name (lowercase, hyphens) or skip
- **Script path rewriting**: `${CLAUDE_SKILL_DIR}/scripts/foo.ts` in SKILL.md needs to be rewritten to `${CLAUDE_SKILL_DIR}/scripts/foo.ts` (path stays relative to skill dir within the plugin)

## Historical Context (from research/)

- `research/docs/2026-03-16-aipm-init-workspace-marketplace.md` — Research for the init/marketplace scaffolding feature
- `research/docs/2026-03-16-claude-code-defaults.md` — Claude Code default settings and configuration
- `research/docs/2026-03-20-30-better-default-plugin.md` — Research for the starter plugin improvements
- `research/docs/2026-03-20-scaffold-plugin-ts-missing-features.md` — Gap analysis for scaffold script features

## Related Research

- `specs/2026-03-09-aipm-technical-design.md` — Overall technical design, adoption path (section 8.2)
- `specs/2026-03-16-aipm-init-workspace-marketplace.md` — Workspace/marketplace init design
- `specs/2026-03-20-better-default-plugin.md` — Starter plugin design with components
- `specs/2026-03-20-scaffold-plugin-registration.md` — Plugin registration in marketplace.json

## Open Questions

- [ ] Should `aipm migrate` auto-create `.ai/` if it doesn't exist, or require `aipm init` first?
- [ ] Should migrated skills be **copied** or **moved** from `.claude/skills/`? (Copy is safer and non-destructive)
- [ ] Should the `Fs` trait be extended with `read_dir`/`list_entries`, or should scanning use a separate abstraction?
- [ ] How should `${CLAUDE_SKILL_DIR}` references in SKILL.md be handled post-migration? (The variable resolves differently in a plugin context vs `.claude/skills/`)
- [ ] Should legacy `.claude/commands/*.md` be treated as skills for migration purposes?
- [ ] Should the `--dry-run` flag be added to the CLI args on day 1 (accepting but not implementing), or added later?
- [ ] What should happen if a skill's name conflicts with an existing plugin in the marketplace?
- [ ] Should the migrated plugin include the skill's frontmatter hooks in the plugin's `hooks.json`?
