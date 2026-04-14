---
date: 2026-04-14 14:55:11 UTC
researcher: Claude (Opus 4.6)
git_commit: 06bc32fe4c1865736864d29c6b37ad68beb26072
branch: main
repository: aipm
topic: "Implementation-ready research for aipm make foundational scaffolding API (#363)"
tags: [research, codebase, aipm-make, scaffolding, atomic-primitives, action-pipeline, starter-plugin, wizard]
status: complete
last_updated: 2026-04-14
last_updated_by: Claude (Opus 4.6)
last_updated_note: "v2 — incorporated user design decisions (7 resolved open questions), updated issue requirements, starter plugin redesign (#361), engine-feature mapping research"
---

# Research: `aipm make` Foundational Scaffolding API (#363) — v2

## Research Question

Create an implementation-ready research document for `aipm make` (#363) by documenting: (1) every existing atomic CRUD primitive in libaipm, (2) the current `generate/` module APIs, (3) the CLI and wizard integration patterns, (4) the engine-to-feature mapping for filtering available options, (5) the starter plugin redesign (#361) as a bash script replacing TypeScript, and (6) the central action dispatching system that will be shared with future `lint --fix`.

## Design Decisions (Resolved)

These 7 decisions were confirmed by the project owner and are **firm constraints** for implementation:

| # | Decision | Rationale |
|---|---|---|
| 1 | **Flat flags** — `aipm make plugin --name foo --engine claude` not nested subcommands | Consistent with all existing commands; no precedent for nested clap subcommands |
| 2 | **Unified central action dispatching system** — shared `Action` type between `aipm make` and future `lint --fix` | Both describe atomic, idempotent filesystem mutations; single type serves both |
| 3 | **Idempotent with logging** — if action already done, log it and inform the user (not a silent no-op) | Users need feedback about what happened; silent no-ops are confusing |
| 4 | **No dry-run** for `aipm make` | Unnecessary complexity for scaffolding; workspace_init/migrate support dry-run because they're destructive |
| 5 | **Scope: `aipm make` only** — `lint --fix` deferred to a subsequent PR | Keep the PR focused; the shared action type enables `--fix` later without rework |
| 6 | **`aipm make extension --engine` deferred** — just support future extensibility | Reserve the command surface; don't implement engine-specific SDK scaffolding yet |
| 7 | **Starter plugin = bash script calling `aipm make` API** — zero TypeScript, replaces #361 | Eliminates Node.js runtime dependency; the SKILL.md instructs LLMs to call `aipm make plugin` |

## Summary

The `aipm make plugin` command will be a new CLI command in the `aipm` binary that composes existing atomic CRUD primitives into an ordered, idempotent scaffolding pipeline. It follows the "action accumulator" pattern already established by `workspace_init::init()` and `migrate::migrate()`: a library function performs ordered filesystem operations, accumulates a `Vec<Action>` enum, and returns it to the CLI layer for rendering.

The command has a wizard counterpart (interactive prompts for engine selection and feature multi-select) that is disabled when the required flags are passed. This follows the existing two-layer wizard pattern: pure prompt definitions in `wizard.rs`, TTY bridge in `wizard_tty.rs`.

The `Action` enum is designed as a central dispatching system shared with future `lint --fix` — both describe atomic, idempotent filesystem mutations. For this PR, only `aipm make` uses it; the lint integration comes later.

The starter plugin (#361) is redesigned: the TypeScript scaffold script (`scaffold-plugin.ts`) is replaced by a bash script (or direct SKILL.md instructions) that calls `aipm make plugin <args>`. This eliminates the Node.js runtime dependency.

**No `aipm.toml` manifests** are generated in the scaffolded plugins for now. Output is pure `.ai/` marketplace format (plugin.json, directory structure).

---

## Scope Boundaries

### In Scope (This PR)
- `aipm make plugin` command with flat flags and wizard counterpart
- Central `Action` enum in a new `make/` module in libaipm
- Engine selection prompt (Claude, Copilot CLI) with flat flag `--engine`
- Multi-select for AI features, filtered by selected engine(s)
- Plugin creation: directories, plugin.json, component templates
- Marketplace registration: add entry to marketplace.json
- Engine settings: enable plugin in engine settings (e.g., `.claude/settings.json`)
- Idempotent behavior with informational logging
- Lint-compliant output (scaffolded files pass `aipm lint`)
- `MultiSelect` variant added to `PromptKind` in shared wizard types
- Starter plugin redesign: bash script replacing TypeScript (#361)

### Out of Scope (Subsequent PRs)
- `lint --fix` integration (action type is designed for it, but not wired up)
- `aipm make extension --engine` (command surface reserved, not implemented)
- `aipm.toml` manifest generation in scaffolded plugins (opt-in later)
- `aipm-pack` integration (pack uses its own `init` command)
- Engine-specific SDK scaffolding (Claude SDK, Copilot CLI config)

---

## `aipm make plugin` Command Design

### CLI Surface

```
aipm make plugin [OPTIONS]

Options:
  --name <NAME>          Plugin name (required or prompted)
  --engine <ENGINE>      Target engine(s): claude, copilot (required or prompted; repeatable)
  --feature <FEATURE>    AI feature types: skill, agent, mcp, hook, lsp, output-style (prompted via multi-select; repeatable)
  -y, --yes              Skip interactive prompts, use defaults
  --dir <DIR>            Working directory (default: current dir)
```

When all required flags are provided (`--name`, `--engine`, `--feature`), the wizard is entirely skipped. When any are missing and stdin is a terminal, the wizard prompts for the missing values. When not interactive and flags are missing, defaults apply (engine: claude, features: skill).

### Wizard Prompts (Interactive Mode)

The wizard follows the existing two-layer pattern. New prompt steps for `aipm make plugin`:

| Step | Kind | Condition to Show | Help Text |
|---|---|---|---|
| Plugin name | Text (with validation) | `--name` not provided | "Lowercase, hyphens allowed" |
| Engine support | Select | `--engine` not provided | "Which AI coding tool(s) will this plugin target?" |
| AI features | **MultiSelect** (NEW) | `--feature` not provided | "Select the AI features to include" |

**Engine options:**
- "Claude Code" (index 0, default)
- "Copilot CLI" (index 1)
- "Both" (index 2)

**Feature options** (filtered by engine selection):

| Feature | Claude | Copilot | Display Label |
|---|---|---|---|
| Skills | yes | yes | "Skills (prompt templates)" |
| Agents | yes | yes | "Agents (autonomous sub-agents)" |
| MCP Servers | yes | yes | "MCP Servers (tool providers)" |
| Hooks | yes | yes | "Hooks (lifecycle events)" |
| Output Styles | yes | no | "Output Styles (response formatting)" |
| LSP Servers | no | yes | "LSP Servers (language intelligence)" |
| Extensions | no | yes | "Extensions (Copilot extensions)" |

When "Both" engines selected, all features are available. When a single engine is selected, only features supported by that engine appear in the multi-select.

### Required New Infrastructure: `MultiSelect` Prompt Kind

The shared `PromptKind` enum (`libaipm::wizard`) currently has 3 variants: `Select`, `Confirm`, `Text`. The `inquire` crate supports `MultiSelect` (confirmed in `research/docs/2026-03-22-rust-interactive-cli-prompts.md`), but it is not yet exposed.

**New variant needed:**
```
MultiSelect {
    options: Vec<&'static str>,
    defaults: Vec<bool>,     // per-option default selection state
}
```

**New answer variant:**
```
MultiSelected(Vec<usize>)   // indices of selected options
```

The TTY layer (`wizard_tty.rs`) needs a new handler that dispatches to `inquire::MultiSelect::new()`.

### Required New Infrastructure: Engine-to-Feature Mapping

No explicit engine-to-feature mapping exists in the codebase. The implicit mapping is derivable from migrate detectors (`migrate/detector.rs:23-44`), but it is not codified as a reusable data structure.

**New data structure needed** (in the `make/` module):

An engine-feature matrix that maps each `Engine` variant to the set of available feature types. This should be a const/static table, not derived from the detector system (which operates at migration time, not creation time).

### Action Pipeline

The `aipm make plugin` library function follows the guard-then-accumulate pattern:

```
make_plugin(opts: &MakePluginOpts, fs: &dyn Fs) -> Result<MakeResult, Error>

Sequence:
1. Validate plugin name (manifest::validate::check_name with ValidationMode::Strict)
2. Resolve target directory (.ai/<plugin-name>/)
3. Guard: if directory already exists -> return idempotent actions with AlreadyExists info
4. Create plugin directory structure based on selected features
5. Write component templates (SKILL.md, agent .md, hooks.json, etc.) per feature
6. Generate and write .claude-plugin/plugin.json
7. Register plugin in marketplace.json (generate::marketplace::register)
8. For each engine: update engine settings (generate::settings::enable_plugin)
9. Return MakeResult { actions: Vec<Action> }
```

### Idempotency Behavior

Each step checks preconditions and either:
- **Executes** the action and records `Action::Completed { ... }` — the normal case
- **Skips** the action and records `Action::AlreadyExists { what, path }` — idempotent duplicate; logged and reported to the user

The CLI layer renders both variants: completed actions with a checkmark, already-exists actions with an info icon and "already exists" message.

### Lint Compliance

All generated files must pass `aipm lint` with zero errors. This means:
- Skills: SKILL.md must have `name` and `description` in frontmatter, description <= 200 chars, name matches directory
- Plugins: plugin.json must have all required fields (name, version, description)
- Hooks: hooks.json must use valid event names (not legacy names)
- Marketplace: marketplace.json entries must have valid plugin references

The generate functions in `generate/plugin_json.rs`, `generate/marketplace.rs`, and the template generators already produce lint-compliant output (the lint rules were designed around the generated format).

---

## Central Action Dispatching System

### Action Enum Design

The `Action` type lives in a new `make/` module and is designed to be shared with future `lint --fix`:

```
pub enum Action {
    // Directory operations
    DirectoryCreated { path: PathBuf },
    DirectoryAlreadyExists { path: PathBuf },

    // File operations
    FileWritten { path: PathBuf, description: String },
    FileAlreadyExists { path: PathBuf },

    // Marketplace operations
    PluginRegistered { name: String, marketplace_path: PathBuf },
    PluginAlreadyRegistered { name: String },

    // Settings operations
    PluginEnabled { plugin_key: String, settings_path: PathBuf },
    PluginAlreadyEnabled { plugin_key: String },

    // Composite
    PluginCreated { name: String, path: PathBuf, features: Vec<String> },
}
```

Each variant captures what happened and where, enabling:
1. **CLI rendering**: human-readable output per action
2. **Programmatic consumption**: the returned `Vec<Action>` can be inspected in tests
3. **Future `lint --fix`**: fix actions map to the same enum (e.g., `FileWritten` for adding a missing frontmatter field)

### Comparison with Existing Action Enums

| System | Enum | Variants | Pattern |
|---|---|---|---|
| workspace_init | `InitAction` | 3 (WorkspaceCreated, MarketplaceCreated, ToolConfigured) | Coarse-grained |
| migrate | `Action` | 10 (PluginCreated, MarketplaceRegistered, Skipped, ...) | Fine-grained |
| **make (new)** | `Action` | ~9 | **Mid-grained — composable for both make and fix** |

The existing `InitAction` and migrate `Action` enums are NOT replaced — they continue to serve their commands. The new `make::Action` is purpose-built for the action dispatching system.

---

## Starter Plugin Redesign (#361)

### Current State (TypeScript)

The starter plugin at `.ai/starter-aipm-plugin/` currently includes:
- `scripts/scaffold-plugin.ts` — 88-line TypeScript file generated as a Rust string literal in `workspace_init/mod.rs:327-416`
- `skills/scaffold-plugin/SKILL.md` — instructs LLMs to run `node --experimental-strip-types .ai/starter-aipm-plugin/scripts/scaffold-plugin.ts <name>`
- `agents/marketplace-scanner.md` — read-only marketplace analysis agent
- `hooks/hooks.json` — PostToolUse logging hook
- `.claude-plugin/plugin.json` — plugin descriptor
- `.mcp.json` — empty MCP servers stub

The TypeScript script does: parse name -> guard existing -> create dirs -> write aipm.toml + SKILL.md + plugin.json -> register in marketplace.json -> enable in settings.json. Requires Node.js >= 22.6.0.

Three E2E tests exercise the script at `crates/aipm/tests/init_e2e.rs:255-311`:
- `scaffold_script_registers_in_marketplace_json` (line 266)
- `scaffold_script_enables_in_settings_json` (line 290)
- `scaffold_script_creates_plugin_directory` (line 313)

### Target State (Bash + `aipm make`)

**The starter plugin should contain zero TypeScript.** Replace `scripts/scaffold-plugin.ts` with a bash script that calls `aipm make plugin`.

**New `scripts/scaffold-plugin.sh`:**
```bash
#!/usr/bin/env bash
set -euo pipefail
# Scaffold a new plugin using the aipm make API
# Usage: ./scaffold-plugin.sh <plugin-name> [--engine claude|copilot]
aipm make plugin --name "${1:?Plugin name required}" --engine "${2:-claude}" -y
```

**Updated `skills/scaffold-plugin/SKILL.md`:**
Instead of instructing the LLM to run a Node.js script, it instructs the LLM to either:
1. Run `aipm make plugin --name <name> --engine <engine> --feature <features> -y` directly (preferred — the LLM can reason about which flags to pass)
2. Or run the bash script as a fallback

The SKILL.md becomes a set of CLI instructions that the LLM can adapt based on the user's request, rather than a fixed script invocation.

### What Changes in `workspace_init/`

| Function | Current | After |
|---|---|---|
| `generate_scaffold_script()` (mod.rs:327-416) | Returns 88-line TypeScript string | Returns ~5-line bash script string |
| `generate_skill_template()` (mod.rs:299-324) | Instructs `node --experimental-strip-types` | Instructs `aipm make plugin` with flag examples |
| `scaffold_marketplace()` writes | `scripts/scaffold-plugin.ts` | `scripts/scaffold-plugin.sh` |
| Snapshot test | `scaffold_script_snapshot.snap` | Updated for bash content |
| E2E tests (init_e2e.rs:255-311) | Invoke `node` with TypeScript | Invoke `aipm make plugin` or bash script |

### Components Preserved (No Change)
- `agents/marketplace-scanner.md` — unchanged
- `hooks/hooks.json` — unchanged
- `.claude-plugin/plugin.json` — unchanged (update component paths if script extension changes)
- `.mcp.json` — unchanged

---

## Atomic Primitives Inventory

These are the existing functions in libaipm that `aipm make plugin` composes. All operate through the `&dyn Fs` abstraction.

### A1. Marketplace CRUD (`generate/marketplace.rs`)

| Function | Signature | Purpose |
|---|---|---|
| `create` | `(marketplace_name: &str, initial_plugins: &[Entry<'_>]) -> String` | Generate fresh marketplace.json content (no I/O) |
| `register` | `(fs: &dyn Fs, path: &Path, entry: &Entry<'_>) -> io::Result<()>` | Add single plugin entry (skip if exists) |
| `register_all` | `(fs: &dyn Fs, path: &Path, entries: &[Entry<'_>]) -> io::Result<()>` | Batch add plugins (single read/write) |
| `unregister` | `(fs: &dyn Fs, path: &Path, plugin_name: &str) -> io::Result<()>` | Remove plugin by name |

Supporting type: `Entry<'a> { name: &'a str, description: &'a str }`

**Current callers:** `create` by workspace_init, `register_all` by migrate/registrar. `register` and `unregister` have no production callers yet (built for `aipm make`).

### A2. Plugin JSON Generation (`generate/plugin_json.rs`)

| Function | Signature | Purpose |
|---|---|---|
| `generate` | `(opts: &Opts<'_>, components: Option<&Components<'_>>) -> String` | Generate plugin.json content (no I/O) |

Supporting types:
- `Opts<'a> { name: &'a str, version: &'a str, description: &'a str }`
- `Components<'a> { skills, agents, mcp_servers, hooks, output_styles, lsp_servers, extensions }` -- all `Option<&'a str>`, derives `Default`

### A3. Settings JSON Read-Modify-Write (`generate/settings.rs`)

| Function | Signature | Purpose |
|---|---|---|
| `read_or_create` | `(fs: &dyn Fs, path: &Path) -> io::Result<Value>` | Read settings or return empty `{}` |
| `write` | `(fs: &dyn Fs, path: &Path, value: &Value) -> io::Result<()>` | Pretty-print JSON + trailing newline |
| `add_known_marketplace` | `(settings: &mut Value, marketplace_name: &str) -> bool` | Idempotent marketplace registration |
| `enable_plugin` | `(settings: &mut Value, plugin_key: &str) -> bool` | Idempotent plugin enablement |

### A4. Manifest TOML Generation (`manifest/builder.rs`)

| Function | Signature | Purpose |
|---|---|---|
| `build_plugin_manifest` | `(opts: &PluginManifestOpts<'_>, components: Option<&PluginComponentsOpts<'_>>) -> String` | Generate `aipm.toml` for plugins (no I/O) |
| `build_workspace_manifest` | `(opts: &WorkspaceManifestOpts<'_>) -> String` | Generate `aipm.toml` for workspaces (no I/O) |

**Note:** `aipm make plugin` does NOT generate `aipm.toml` in the initial scope (per "NO `aipm.toml` or proprietary stuff yet" requirement). These builders are available for future opt-in.

### A5. Plugin Package Scaffolding (`init.rs`)

| Function | Signature | Purpose |
|---|---|---|
| `init` | `(opts: &Options<'_>, fs: &dyn Fs) -> Result<(), Error>` | Full plugin package scaffolding (validate + directories + manifest) |

Private internals that `aipm make` may decompose:
- `create_directory_layout(dir, plugin_type, fs)` -- creates type-specific subdirectories
- `create_skill_template(dir, fs)` -- writes `skills/default/SKILL.md`
- `create_gitkeep(dir, fs)` -- writes `.gitkeep`
- `generate_manifest(name, plugin_type)` -- calls `build_plugin_manifest`

### A6. Workspace Scaffolding (`workspace_init/mod.rs`)

| Function | Signature | Purpose |
|---|---|---|
| `init` | `(opts: &Options<'_>, adaptors: &[Box<dyn ToolAdaptor>], fs: &dyn Fs) -> Result<InitResult, Error>` | Full workspace + marketplace scaffolding |

This is the primary pattern model for `aipm make` -- already uses the action accumulator pattern.

### A7. Name Validation (`manifest/validate.rs`)

| Function | Signature | Purpose |
|---|---|---|
| `is_valid_name` | `(name: &str, mode: ValidationMode) -> bool` | Validate package/marketplace name |
| `check_name` | `(name: &str, mode: ValidationMode) -> Result<(), String>` | Validate with human-readable error message |

### A8-A12. Supporting Primitives

| Module | Key Functions | Relevance to `aipm make` |
|---|---|---|
| `installer/manifest_editor.rs` | `add_dependency`, `remove_dependency` | Not used by `make plugin` initially |
| `linker/gitignore.rs` | `add_entry`, `remove_entry`, `read_entries` | May be used for `.gitignore` management |
| `linker/link_state.rs` | `add`, `remove`, `list`, `clear_all` | Not used by `make plugin` |
| `linker/directory_link.rs` | `create`, `remove`, `is_link`, `read_target` | Not used by `make plugin` |
| `fs.rs` | `Fs` trait (14 methods), `write_file_with_parents`, `read_or_default<T>` | Foundation for all I/O |

---

## Action Accumulator Pattern (Model for `aipm make`)

Both `workspace_init::init()` and `migrate::migrate()` follow the pattern `aipm make` should use:

```
Library layer:
  1. Validate preconditions
  2. Execute ordered filesystem operations
  3. Accumulate Vec<ActionEnum> recording each completed step
  4. Return the action list as the sole result

CLI layer:
  1. Call library function
  2. Iterate returned actions
  3. Map each action variant to a human-readable writeln!() message
  4. Return Ok/Err for exit code
```

### workspace_init Actions (`workspace_init/mod.rs:71-79`)

```rust
pub enum InitAction {
    WorkspaceCreated,
    MarketplaceCreated,
    ToolConfigured(String),
}
```

### migrate Actions (`migrate/mod.rs:135-205`)

```rust
pub enum Action {
    PluginCreated { name, source, plugin_type, source_is_dir },
    MarketplaceRegistered { name },
    Renamed { original_name, new_name, reason },
    Skipped { name, reason },
    DryRunReport { path },
    SourceFileRemoved { path },
    SourceDirRemoved { path },
    EmptyDirPruned { path },
    OtherFileMigrated { path, destination, associated_artifact },
    ExternalReferenceDetected { path, referenced_by },
}
```

---

## Engine/Adaptor System

### Current State

| Component | Location | Status |
|---|---|---|
| `Engine` enum | `engine.rs:14` | 2 variants: `Claude`, `Copilot` |
| `ToolAdaptor` trait | `workspace_init/mod.rs:20-48` | Only Claude adaptor implemented |
| `Package.engines` field | `manifest/types.rs:66` | Declaration in aipm.toml, Optional |
| Engine-feature mapping | **Does not exist** | Must be created for `aipm make` |

### Engine-Feature Matrix (To Be Codified)

Derived from migrate detectors (`migrate/detector.rs:23-44`) and the `ArtifactKind` enum (`migrate/mod.rs:33-66`):

| Feature Type | Claude | Copilot | PluginType Mapping |
|---|---|---|---|
| Skills | yes | yes | `Skill` |
| Agents | yes | yes | `Agent` |
| MCP Servers | yes | yes | `Mcp` |
| Hooks | yes | yes | `Hook` |
| Output Styles | yes | no | (part of `Composite`) |
| LSP Servers | no | yes | `Lsp` |
| Extensions | no | yes | (part of `Composite`) |
| Commands (legacy) | yes | no | (migrated to Skills) |

This matrix needs to be a concrete data structure in the `make/` module, not derived at runtime from detectors.

### PluginType Determination

The `PluginType` for the generated plugin is derived from the feature selection:
- Single feature type selected -> that type (e.g., `Skill`, `Agent`, `Mcp`, `Hook`, `Lsp`)
- Multiple feature types selected -> `Composite`
- This matches the existing `init.rs` behavior where `PluginType::Composite` creates multiple component directories

---

## Wizard System Integration

### Existing Two-Layer Architecture

**Pure layer** (`wizard.rs`):
- `*_prompt_steps(flag_a, flag_b, ...)` -> `Vec<PromptStep>` — builds prompts, skipping those whose flags are set
- `resolve_*_answers(answers, flag_a, flag_b, ...)` -> resolved values — consumes answer array in same conditional order

**TTY layer** (`wizard_tty.rs`):
- `resolve(interactive, flags)` -> calls pure layer, executes prompts via `inquire`, resolves answers
- `execute_prompts(steps)` -> dispatches each `PromptKind` to the appropriate `inquire` widget
- Interactivity: `!yes && stdin.is_terminal()`

### New Wizard Functions Needed

**In `crates/aipm/src/wizard.rs`:**
- `make_plugin_prompt_steps(flag_name, flag_engine, flag_features)` -> `Vec<PromptStep>`
- `resolve_make_plugin_answers(answers, flag_name, flag_engine, flag_features)` -> `(name, engines, features)`
- `resolve_make_plugin_defaults(flag_name, flag_engine, flag_features)` -> defaults for non-interactive

**In `crates/libaipm/src/wizard.rs`:**
- New `PromptKind::MultiSelect { options, defaults }` variant
- New `PromptAnswer::MultiSelected(Vec<usize>)` variant

**In `crates/aipm/src/wizard_tty.rs`:**
- New handler in `execute_prompts()` for `PromptKind::MultiSelect` dispatching to `inquire::MultiSelect::new()`

### Feature Filtering in the Wizard

The multi-select options list depends on the engine selection answer. This creates a **dependency between wizard steps**: the engine answer must be resolved before the feature options can be constructed.

Approaches:
1. **Two-phase wizard**: Execute engine prompt first, then dynamically construct feature prompt. This breaks the current single-pass `execute_prompts()` pattern but is the most natural UX.
2. **Pre-filtered step construction**: If engine is provided via `--engine` flag, construct feature options at step-build time. If engine is prompted, use a two-phase approach.

The two-phase approach is consistent with how `workspace_init` already uses conditional logic — the marketplace name prompt depends on the setup mode answer.

---

## New Module Structure

```
crates/libaipm/src/
  make/
    mod.rs          -- public API: make_plugin(), MakePluginOpts, MakeResult
    action.rs       -- Action enum (central dispatching type)
    error.rs        -- Error enum for make operations
    engine_features.rs  -- Engine-to-feature mapping matrix
    templates.rs    -- Component template generators (SKILL.md, agent.md, hooks.json, etc.)

crates/aipm/src/
  main.rs           -- Add Make variant to Commands enum
  wizard.rs         -- Add make_plugin_prompt_steps(), resolve_make_plugin_answers()
  wizard_tty.rs     -- Add MultiSelect handler in execute_prompts()
```

### What Exists vs What's New

```
Exists (available as primitives):          New for #363:
  generate/marketplace.rs                    make/mod.rs
  generate/plugin_json.rs                    make/action.rs
  generate/settings.rs                       make/error.rs
  manifest/builder.rs                        make/engine_features.rs
  manifest/validate.rs                       make/templates.rs
  init.rs                                    wizard.rs (MultiSelect variant)
  workspace_init/mod.rs                      wizard_tty.rs (MultiSelect handler)
  workspace_init/adaptors/claude.rs          main.rs (Make command + cmd_make_plugin)
  fs.rs
  wizard.rs (shared types)
```

---

## Code References

### Atomic Primitive Modules
- [`crates/libaipm/src/generate/mod.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/generate/mod.rs) -- module hub
- [`crates/libaipm/src/generate/marketplace.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/generate/marketplace.rs) -- marketplace CRUD
- [`crates/libaipm/src/generate/plugin_json.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/generate/plugin_json.rs) -- plugin.json generation
- [`crates/libaipm/src/generate/settings.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/generate/settings.rs) -- settings.json RMW
- [`crates/libaipm/src/manifest/builder.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/manifest/builder.rs) -- TOML manifest builder
- [`crates/libaipm/src/init.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/init.rs) -- plugin package scaffolding
- [`crates/libaipm/src/manifest/validate.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/manifest/validate.rs) -- name validation

### Action Accumulator Pattern Models
- [`crates/libaipm/src/workspace_init/mod.rs:71-119`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/workspace_init/mod.rs#L71) -- InitAction enum + init() orchestrator
- [`crates/libaipm/src/workspace_init/mod.rs:174-273`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/workspace_init/mod.rs#L174) -- scaffold_marketplace() ordered sequence
- [`crates/libaipm/src/migrate/mod.rs:135-205`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/migrate/mod.rs#L135) -- Action enum (10 variants)
- [`crates/libaipm/src/migrate/mod.rs:260-566`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/migrate/mod.rs#L260) -- migrate() orchestrator

### CLI Integration Points
- [`crates/aipm/src/main.rs:32-217`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm/src/main.rs#L32) -- Commands enum (10 variants, flat structure)
- [`crates/aipm/src/main.rs:322-327`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm/src/main.rs#L322) -- InitWizardFlags struct (model for MakeWizardFlags)
- [`crates/aipm/src/main.rs:329-376`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm/src/main.rs#L329) -- cmd_init() action rendering pattern
- [`crates/aipm/src/main.rs:938-1007`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm/src/main.rs#L938) -- run() dispatch block

### Wizard System
- [`crates/libaipm/src/wizard.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/wizard.rs) -- PromptStep, PromptKind (3 variants), PromptAnswer (3 variants), styled_render_config()
- [`crates/aipm/src/wizard.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm/src/wizard.rs) -- workspace_prompt_steps(), resolve_workspace_answers()
- [`crates/aipm/src/wizard_tty.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm/src/wizard_tty.rs) -- execute_prompts(), resolve()

### Engine/Adaptor System
- [`crates/libaipm/src/engine.rs:14`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/engine.rs#L14) -- Engine enum (Claude, Copilot)
- [`crates/libaipm/src/workspace_init/adaptors/claude.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/workspace_init/adaptors/claude.rs) -- Claude adaptor
- [`crates/libaipm/src/workspace_init/adaptors/mod.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/workspace_init/adaptors/mod.rs) -- defaults() factory
- [`crates/libaipm/src/manifest/types.rs:251`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/manifest/types.rs#L251) -- PluginType enum (6 variants)
- [`crates/libaipm/src/manifest/types.rs:158`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/manifest/types.rs#L158) -- Components struct (9 fields)
- [`crates/libaipm/src/migrate/detector.rs:23-44`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/migrate/detector.rs#L23) -- Implicit engine-to-feature mapping

### Starter Plugin (Current TypeScript)
- [`crates/libaipm/src/workspace_init/mod.rs:327-416`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/workspace_init/mod.rs#L327) -- generate_scaffold_script() (TypeScript generator)
- [`crates/libaipm/src/workspace_init/mod.rs:299-324`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/workspace_init/mod.rs#L299) -- generate_skill_template() (SKILL.md generator)
- [`crates/libaipm/src/workspace_init/mod.rs:418-448`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/workspace_init/mod.rs#L418) -- generate_agent_template()
- [`crates/libaipm/src/workspace_init/mod.rs:450-460`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/workspace_init/mod.rs#L450) -- generate_hook_template()
- [`.ai/starter-aipm-plugin/scripts/scaffold-plugin.ts`](https://github.com/TheLarkInn/aipm/blob/06bc32f/.ai/starter-aipm-plugin/scripts/scaffold-plugin.ts) -- On-disk TypeScript (to be replaced)
- [`crates/aipm/tests/init_e2e.rs:255-311`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm/tests/init_e2e.rs#L255) -- E2E tests for scaffold script

### Extension Points
- [`crates/libaipm/src/workspace_init/mod.rs:19-48`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/workspace_init/mod.rs#L19) -- ToolAdaptor trait
- [`crates/libaipm/src/fs.rs:27`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/fs.rs#L27) -- Fs trait (14 methods)

---

## Architecture Documentation

### Patterns That `aipm make` Follows

**Action accumulator:** Library function returns `Vec<ActionEnum>`, CLI renders each variant. Used by workspace_init and migrate.

**Guard-then-accumulate:** Precondition checks first, then sequential filesystem operations, each appending to the action vector.

**Two-layer wizard:** Pure prompt definitions (snapshot-testable) + thin TTY bridge. Shared types in `libaipm::wizard`.

**Flag-based step omission:** Each wizard `*_prompt_steps()` function checks flags and conditionally includes/excludes prompts. `resolve_*_answers()` consumes answers in the same conditional order.

**Filesystem abstraction:** All production code uses `&dyn Fs`. `Real` struct for production, `MockFs` for tests.

**Adaptor plug-in points:** `ToolAdaptor` trait for engine-specific post-scaffolding. Only Claude implemented; Copilot is a future addition.

---

## Historical Context (from research/)

- [`research/docs/2026-04-12-dry-rust-architecture-audit.md`](https://github.com/TheLarkInn/aipm/blob/06bc32f/research/docs/2026-04-12-dry-rust-architecture-audit.md) -- DRY audit (prerequisite, complete). All 28 features across 4 phases resolved.
- [`research/progress.txt`](https://github.com/TheLarkInn/aipm/blob/06bc32f/research/progress.txt) -- Execution log of DRY consolidation.
- [`research/tickets/2026-04-14-0417-merge-pack-into-aipm.md`](https://github.com/TheLarkInn/aipm/blob/06bc32f/research/tickets/2026-04-14-0417-merge-pack-into-aipm.md) -- Pack merge analysis. #361 directly depends on #363.
- [`research/docs/2026-03-31-110-aipm-lint-architecture-research.md`](https://github.com/TheLarkInn/aipm/blob/06bc32f/research/docs/2026-03-31-110-aipm-lint-architecture-research.md) -- Lint architecture noting "No `--fix` auto-fix infrastructure."
- [`research/docs/2026-03-20-scaffold-plugin-ts-missing-features.md`](https://github.com/TheLarkInn/aipm/blob/06bc32f/research/docs/2026-03-20-scaffold-plugin-ts-missing-features.md) -- TypeScript scaffold gaps (#361).
- [`research/docs/2026-03-22-rust-interactive-cli-prompts.md`](https://github.com/TheLarkInn/aipm/blob/06bc32f/research/docs/2026-03-22-rust-interactive-cli-prompts.md) -- Confirms `inquire` crate supports MultiSelect.
- [`research/docs/2026-03-19-init-tool-adaptor-refactor.md`](https://github.com/TheLarkInn/aipm/blob/06bc32f/research/docs/2026-03-19-init-tool-adaptor-refactor.md) -- ToolAdaptor design history.

---

## Related Research

- `research/docs/2026-04-12-dry-rust-architecture-audit.md` -- DRY audit (prerequisite, complete)
- `research/tickets/2026-04-14-0417-merge-pack-into-aipm.md` -- Pack merge analysis
- `research/docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md` -- Manifest generation paths
- `research/docs/2026-03-19-init-tool-adaptor-refactor.md` -- ToolAdaptor design
- `research/docs/2026-04-06-feature-status-audit.md` -- Feature status audit
- `research/docs/2026-03-22-rust-interactive-cli-prompts.md` -- Interactive CLI library comparison

---

## Open Questions (All Resolved)

1. ~~**Engine prompt widget:** Should engine selection use `Select` or `MultiSelect`?~~ **Resolved.** Select with "Both" option (3 choices: Claude Code, Copilot CLI, Both).

2. ~~**Marketplace location discovery:** Walk up, cwd, or --dir flag?~~ **Resolved.** Walk up from cwd looking for `.ai/`.

3. ~~**Template content per feature:** Minimal or rich templates?~~ **Resolved.** Minimal lint-passing templates with required frontmatter fields and placeholder values.

4. ~~**Copilot adaptor scope:** Implement or defer?~~ **Resolved.** Accept `--engine copilot` but only scaffold `.ai/` marketplace structure — no Copilot-specific settings generation.

5. ~~**On-disk drift:** Fix in this PR or separately?~~ **Resolved.** Fix in this PR as part of the starter plugin redesign.

6. **Command shape:** `plugin` is a positional arg to `make` (`aipm make plugin --name foo`), not a nested subcommand or flag value.

7. **Default features with -y:** Error if `--feature` missing in non-interactive mode — no guessing.

**Spec:** See `specs/2026-04-14-aipm-make-plugin-command.md` for the full technical design document.
