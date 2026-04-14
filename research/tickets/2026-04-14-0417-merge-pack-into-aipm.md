---
date: 2026-04-14 14:34:34 UTC
researcher: Claude (Opus 4.6)
git_commit: 06bc32fe4c1865736864d29c6b37ad68beb26072
branch: main
repository: aipm
topic: "Merge aipm-pack into aipm (#417) with dependency analysis on aipm make (#363)"
tags: [research, codebase, aipm-pack, aipm-make, cli-consolidation, scaffolding, binary-merge]
status: complete
last_updated: 2026-04-14
last_updated_by: Claude (Opus 4.6)
---

# Research: Merge aipm-pack into aipm (#417) with #363 Dependency Analysis

## Research Question

Analyze the current `aipm-pack` crate structure, its commands, shared code with `aipm`, and what merging them (#417) would entail -- including binary consolidation, command routing, shared dependencies, and impact on tests/CI. Determine whether implementing `aipm make` (#363) -- the foundational scaffolding API that would make `aipm-pack init` obsolete -- should be implemented before, after, or concurrently with the pack merge.

## Summary

`aipm-pack` is a thin CLI binary (~980 lines of source, excluding snapshots) that exposes a single `init` subcommand for scaffolding plugin packages. Every dependency it uses is already present in `aipm`. The merge (#417) is mechanically straightforward: move the `Init` command variant and its wizard into the `aipm` CLI under a `pack init` subcommand (or a top-level `init --pack` flag), delete the `crates/aipm-pack/` directory, and update the release pipeline.

However, issue #417 itself states: "With the 'aipm make' api also on backlog, this will make 'aipm-pack init' obsolete." This signals that the `aipm make` API (#363) would subsume `aipm-pack init` entirely -- meaning the merge target may not need to exist if #363 ships first. A prior DRY architecture audit (2026-04-12) already identified 10 deduplication targets that serve as prerequisite consolidation for both issues.

The analysis below documents the current state and presents the ordering trade-offs.

---

## Detailed Findings

### A. aipm-pack Crate: Current State

**Source files and line counts:**

| File | Lines | Purpose |
|---|---|---|
| [`main.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm-pack/src/main.rs) | 84 | Binary entrypoint, CLI struct, arg parsing, orchestration |
| [`error.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm-pack/src/error.rs) | 73 | Unified `CliError` enum with `From` impls |
| [`wizard.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm-pack/src/wizard.rs) | 380 | Pure-function prompt definitions and answer resolution |
| [`wizard_tty.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm-pack/src/wizard_tty.rs) | 98 | TTY bridge executing prompts via `inquire` |
| [`tests/init_e2e.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm-pack/tests/init_e2e.rs) | 345 | 17 E2E tests via `assert_cmd` |
| **Total source** | **980** | (excluding 11 insta snapshot files) |

**Single command:** `aipm-pack init [dir] [--yes] [--name NAME] [--type TYPE]`

**Data flow:**
1. `main()` -> `run()` -> `Cli::parse()` (clap)
2. Parses `--type` string to `PluginType` via `FromStr`
3. Resolves `"."` to `current_dir()`
4. `wizard_tty::resolve()` -> `wizard::package_prompt_steps()` -> `execute_prompts()` -> `wizard::resolve_package_answers()`
5. Constructs `libaipm::init::Options`, calls `libaipm::init::init(&opts, &libaipm::fs::Real)`
6. Writes success message to stdout

**libaipm APIs consumed (5 total):**
- `libaipm::init::{Options, init, Error}` -- plugin package scaffolding
- `libaipm::manifest::types::PluginType` -- type enum
- `libaipm::fs::Real` -- filesystem implementation
- `libaipm::version()` -- version string
- `libaipm::wizard::*` types (re-exported through local wizard.rs)

### B. aipm CLI Crate: Current State

**Source files and line counts:**

| File | Lines | Purpose |
|---|---|---|
| [`main.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm/src/main.rs) | 1273 | CLI definition, 10 subcommands, command handlers |
| [`error.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm/src/error.rs) | 127 | `CliError` enum with 13 variants |
| [`wizard.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm/src/wizard.rs) | 538 | Pure prompt logic for `init` and `migrate` wizards |
| [`wizard_tty.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm/src/wizard_tty.rs) | 121 | TTY bridge for prompts |
| [`lsp.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm/src/lsp.rs) | 328 | Async LSP server |
| [`lsp/helpers.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm/src/lsp/helpers.rs) | 589 | Sync helpers for LSP |
| **Total** | **2976** | |

**10 subcommands:** `init`, `install`, `update`, `link`, `uninstall`, `unlink`, `list`, `lint`, `migrate`, `lsp`

**Pattern for adding commands:** Add variant to `Commands` enum (lines 32-217), write `cmd_<name>()` handler, add match arm in `run()` (lines 949-1007), add error variant to `CliError` if needed.

**libaipm APIs consumed (~20+ modules):** workspace_init, installer::pipeline, linker::*, lockfile, lint, migrate, logging, fs::Real, manifest::load, workspace::find_workspace_root, cache::Policy, locked_file::LockedFile, installed::Registry, and more.

### C. Dependency Overlap

**Every dependency in aipm-pack is already in aipm:**

| Dependency | aipm-pack | aipm |
|---|---|---|
| `libaipm` (wizard feature) | Yes | Yes |
| `clap` | Yes | Yes |
| `inquire` | Yes | Yes |
| `serde` | Yes | Yes |
| `serde_json` | Yes | Yes |
| `thiserror` | Yes | Yes |
| `toml` | Yes | Yes |
| `tracing` | Yes | Yes |
| `tracing-subscriber` | Yes | Yes |
| `clap-verbosity-flag` | No | Yes (unique to aipm) |
| `tokio` | No | Yes (unique to aipm) |
| `tower-lsp` | No | Yes (unique to aipm) |

Merging adds zero new dependencies to the `aipm` binary.

### D. Shared Wizard Infrastructure (Duplicated)

Both binaries independently contain a two-layer wizard architecture:

| Layer | aipm-pack | aipm |
|---|---|---|
| Pure prompt logic | `wizard.rs` (380 lines) | `wizard.rs` (538 lines) |
| TTY bridge | `wizard_tty.rs` (98 lines) | `wizard_tty.rs` (121 lines) |

Both re-export `libaipm::wizard::{PromptStep, PromptKind, PromptAnswer, styled_render_config}`. Both implement the same `execute_prompts()` function pattern that dispatches `PromptKind::Text`, `PromptKind::Confirm`, and `PromptKind::Select` to `inquire` types. The DRY audit (B7) identified this as a deduplication target.

### E. Release Pipeline Impact

The current release pipeline handles two binaries:

1. **cargo-dist** (`dist-workspace.toml`): Builds both `aipm` and `aipm-pack` for 4 platform targets (x86_64-linux, x86_64-darwin, aarch64-darwin, x86_64-windows)
2. **release-plz** (`release-plz.toml`): Manages separate changelogs and crates.io publishes for all 3 crates
3. **Stable installer** (`update-latest-release.yml`): Only copies `aipm-v*` installers (not `aipm-pack-v*`)
4. **CI** (`ci.yml`): Builds and tests the full workspace

Merging would:
- Remove `crates/aipm-pack/` as a workspace member from `Cargo.toml`
- Eliminate one binary from cargo-dist builds (halving build matrix for that artifact)
- Remove one changelog and one crates.io package
- Simplify the install path (users only need one binary)

### F. What the Merge (#417) Entails Mechanically

1. Add a `Pack` subcommand (or `PackInit`) to `aipm`'s `Commands` enum
2. Move `aipm-pack`'s wizard prompt steps (`package_prompt_steps`, `resolve_package_answers`, `plugin_type_from_index`, `PLUGIN_TYPE_OPTIONS`) into `aipm`'s `wizard.rs` or into `libaipm`
3. Add a `cmd_pack_init()` handler in `aipm/main.rs` calling `libaipm::init::init()`
4. Add `Init(libaipm::init::Error)` variant to `aipm`'s `CliError`
5. Move the 17 E2E tests to `crates/aipm/tests/pack_init_e2e.rs`, retargeting `aipm pack init` instead of `aipm-pack init`
6. Delete `crates/aipm-pack/`
7. Update `Cargo.toml` workspace members
8. Update cargo-dist and release-plz configuration
9. Update README, install guides, and docs

Estimated scope: ~500 lines of code moved/adapted, ~200 lines deleted.

### G. libaipm Scaffolding: The Foundation for aipm make (#363)

Issue #363 proposes raising existing atomic CRUD operations into a callable internal API. These operations already exist scattered across libaipm:

| Operation | Current Location | Module |
|---|---|---|
| Create plugin directory tree | `init::create_directory_layout` | [`init.rs:108-135`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/init.rs#L108) |
| Generate `aipm.toml` | 4 independent paths | `init.rs`, `workspace_init/mod.rs`, `migrate/emitter.rs`, `manifest/builder.rs` |
| Generate `plugin.json` | `generate::plugin_json::generate()` | [`generate/plugin_json.rs:42`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/generate/plugin_json.rs#L42) |
| Register in `marketplace.json` | `generate::marketplace::{create,register,register_all,unregister}` | [`generate/marketplace.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/generate/marketplace.rs) |
| Enable in `settings.json` | `generate::settings::{read_or_create,add_known_marketplace,enable_plugin}` | [`generate/settings.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/generate/settings.rs) |
| Write templates (SKILL.md, agents, hooks) | inline in `init.rs` and `workspace_init/mod.rs` | scattered |
| Validate plugin name | `manifest::validate::check_name()` | [`manifest/validate.rs:38`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/manifest/validate.rs#L38) |
| Manage gitignore entries | `linker::gitignore::{add_entry,remove_entry}` | [`linker/gitignore.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/linker/gitignore.rs) |
| Manage manifest deps | `installer::manifest_editor::{add_dependency,remove_dependency}` | [`installer/manifest_editor.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/installer/manifest_editor.rs) |
| Manage link state | `linker::link_state::{add,remove,list}` | [`linker/link_state.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/linker/link_state.rs) |

**Key gap:** There is **no `IsFixable` trait** in the lint system. The `Rule` trait's `check_file()` returns `Vec<Diagnostic>` only -- diagnostics are output-only with no fix actions. `aipm lint --fix` (mentioned in #363) would require extending the `Rule` trait or adding a parallel `Fix` trait.

**DRY audit pre-work (from 2026-04-12 audit):** The audit identified 10 deduplication targets that should be consolidated before #363:
1. B1: Unify 4 `aipm.toml` generation paths into `manifest::builder`
2. B2: Unify 2 `plugin.json` generation paths into `generate::plugin_json`
3. B3: Unify 4 name validators into `manifest::validate::check_name()`
4. B4: Unify marketplace.json RMW into `generate::marketplace`
5. B5: Unify settings.json RMW into `generate::settings`
6. B6: Unify frontmatter parsing into `frontmatter.rs`
7. B7: Consolidate shared wizard types into `libaipm::wizard`
8. B8: Consolidate "create parent dir then write" into `Fs::write_file_with_parents`
9. B9: Consolidate "read or default" into `fs::read_or_default`/`fs::read_toml_or_default`
10. B10: Eliminate lint rule check()/check_file() duplication

**Progress:** The `feature-list.json` and `progress.txt` in `research/` indicate that some of this DRY consolidation has already been completed (manifest builder unification, plugin.json consolidation, marketplace/settings module extraction, wizard type sharing). The current codebase at commit `06bc32f` reflects these improvements with the `generate/` module containing unified `marketplace.rs`, `plugin_json.rs`, and `settings.rs` submodules, and the `manifest/builder.rs` module handling TOML generation.

### H. Existing Test Coverage for Atomic Operations

The codebase has extensive test coverage that would need to be preserved through either change:

- **102 `#[cfg(test)]` modules** across the workspace
- **31 BDD feature files** in `tests/features/` covering all CLI commands
- **17 E2E tests** for `aipm-pack init` specifically
- **4 MockFs implementations** for testing CRUD operations without real filesystem
- **E2E test helpers** in `migrate_e2e.rs`: `create_skill()`, `create_command()`, `create_agent()`, `create_mcp_json()`, `create_hooks_settings()`, `create_output_style()`
- **BDD step definitions** in `bdd.rs`: 12 GIVEN steps, 3 WHEN steps, 9 THEN steps for CRUD operations

---

## Ordering Analysis: #363 vs #417

### Option 1: Implement #363 (aipm make) First

**Arguments:**
- Issue #417 explicitly states: "With the 'aipm make' api also on backlog, this will make 'aipm-pack init' obsolete." This signals `aipm make` is the intended replacement for `aipm-pack init`.
- If `aipm make plugin new` replaces `aipm-pack init`, then #417 becomes trivial -- just delete `crates/aipm-pack/` and update the release pipeline. No command migration needed.
- The DRY consolidation (pre-work for #363) benefits the entire codebase, including making any future merge cleaner.
- Issue #361 (replace TypeScript scaffold with CLI) depends directly on #363 -- implementing #363 first unblocks #361 as well.
- Avoids "move code then immediately refactor it" -- merging pack's init command into aipm only to replace it with `aipm make` would be wasted motion.

**Risks:**
- #363 is significantly larger in scope than #417. It requires designing a new action/primitive API, extending the lint system with fix capabilities, and building a new CLI command surface.
- Delays the simplification of having a single binary.

### Option 2: Implement #417 (merge pack) First

**Arguments:**
- Mechanically simple: ~500 lines moved, ~200 deleted, zero new dependencies. Low risk, high confidence.
- Simplifies development workflow immediately -- one binary to build, test, and release.
- Reduces cargo-dist build matrix and release complexity before tackling #363.
- The merged `aipm pack init` command can later be aliased to or replaced by `aipm make plugin new` when #363 ships.
- Single binary simplifies user onboarding right now.

**Risks:**
- The merged init command will need refactoring when `aipm make` lands (the wizard, error types, and command routing would change).
- The shared wizard code (currently duplicated) would need to be merged now, then potentially restructured again for `aipm make`'s more complex interactive flows.

### Option 3: Implement Concurrently

**Arguments:**
- #417 touches the CLI layer (binary consolidation), #363 touches the library layer (action API in libaipm). They could proceed in parallel on separate branches.

**Risks:**
- High probability of merge conflicts in `wizard.rs`, `wizard_tty.rs`, `error.rs`, and `main.rs`.
- The wizard consolidation work (DRY item B7) is a prerequisite for both and would need to land first.
- Coordination overhead likely exceeds the time saved.

### Option 4: DRY Consolidation First, Then #417, Then #363

**Arguments:**
- The DRY audit pre-work is valuable regardless of ordering -- it reduces surface area and eliminates duplicated code.
- After DRY consolidation, the pack merge becomes even simpler (shared wizard types already in libaipm, unified validators, unified generators).
- Then #363 builds on a cleaner foundation with all primitives already consolidated.

**Risks:**
- Three sequential phases means slower delivery of visible features.
- However, the DRY consolidation is partially done already per `progress.txt`.

---

## Code References

### aipm-pack Crate
- [`crates/aipm-pack/Cargo.toml`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm-pack/Cargo.toml) -- 9 runtime deps, all subset of aipm
- [`crates/aipm-pack/src/main.rs:17-43`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm-pack/src/main.rs#L17) -- CLI struct and Commands enum
- [`crates/aipm-pack/src/main.rs:46-75`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm-pack/src/main.rs#L46) -- `run()` orchestration function
- [`crates/aipm-pack/src/wizard.rs:43-121`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm-pack/src/wizard.rs#L43) -- prompt steps and answer resolution
- [`crates/aipm-pack/src/error.rs:7-20`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm-pack/src/error.rs#L7) -- CliError enum (3 variants)
- [`crates/aipm-pack/tests/init_e2e.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm-pack/tests/init_e2e.rs) -- 17 E2E tests

### aipm Crate
- [`crates/aipm/src/main.rs:32-217`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm/src/main.rs#L32) -- Commands enum (10 variants)
- [`crates/aipm/src/main.rs:938-1007`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm/src/main.rs#L938) -- `run()` command dispatch
- [`crates/aipm/src/error.rs:9-61`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm/src/error.rs#L9) -- CliError enum (13 variants)
- [`crates/aipm/src/wizard.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/aipm/src/wizard.rs) -- workspace init and migrate wizard prompts

### libaipm Scaffolding APIs
- [`crates/libaipm/src/init.rs:57`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/init.rs#L57) -- `pub fn init()` (plugin package scaffolding)
- [`crates/libaipm/src/workspace_init/mod.rs:96`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/workspace_init/mod.rs#L96) -- `pub fn init()` (workspace + marketplace scaffolding)
- [`crates/libaipm/src/manifest/builder.rs:53`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/manifest/builder.rs#L53) -- `build_plugin_manifest()`
- [`crates/libaipm/src/generate/marketplace.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/generate/marketplace.rs) -- marketplace CRUD
- [`crates/libaipm/src/generate/plugin_json.rs:42`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/generate/plugin_json.rs#L42) -- `generate()`
- [`crates/libaipm/src/generate/settings.rs`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/generate/settings.rs) -- settings CRUD

### Traits
- [`crates/libaipm/src/fs.rs:27`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/fs.rs#L27) -- `Fs` trait (14 methods)
- [`crates/libaipm/src/lint/rule.rs:16`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/lint/rule.rs#L16) -- `Rule` trait (no fix capability)
- [`crates/libaipm/src/workspace_init/mod.rs:20`](https://github.com/TheLarkInn/aipm/blob/06bc32f/crates/libaipm/src/workspace_init/mod.rs#L20) -- `ToolAdaptor` trait

### Release Pipeline
- [`Cargo.toml:2`](https://github.com/TheLarkInn/aipm/blob/06bc32f/Cargo.toml#L2) -- workspace members glob
- [`dist-workspace.toml`](https://github.com/TheLarkInn/aipm/blob/06bc32f/dist-workspace.toml) -- cargo-dist config (builds both binaries)
- [`release-plz.toml`](https://github.com/TheLarkInn/aipm/blob/06bc32f/release-plz.toml) -- release automation
- [`.github/workflows/ci.yml`](https://github.com/TheLarkInn/aipm/blob/06bc32f/.github/workflows/ci.yml) -- CI pipeline
- [`.github/workflows/release.yml`](https://github.com/TheLarkInn/aipm/blob/06bc32f/.github/workflows/release.yml) -- cargo-dist release

---

## Architecture Documentation

### Current Two-Binary Architecture

```
User installs:
  aipm        -- consumer CLI (init, install, update, link, unlink, list, lint, migrate, lsp)
  aipm-pack   -- author CLI (init)

Both depend on:
  libaipm     -- shared library (27+ public modules)
    features = ["wizard"] gates inquire-dependent shared types

Binary output:
  cargo-dist builds 2 binaries x 4 platforms = 8 artifacts per release
  release-plz publishes 3 crates to crates.io (aipm, aipm-pack, libaipm)
```

### Post-Merge Architecture (#417)

```
User installs:
  aipm        -- unified CLI (init, pack init, install, update, link, unlink, list, lint, migrate, lsp)

Depends on:
  libaipm     -- shared library (unchanged)

Binary output:
  cargo-dist builds 1 binary x 4 platforms = 4 artifacts per release
  release-plz publishes 2 crates to crates.io (aipm, libaipm)
```

### Post-Make Architecture (#363 + #417)

```
User installs:
  aipm        -- unified CLI (init, make, install, update, link, unlink, list, lint, migrate, lsp)

  aipm make plugin new     -- replaces aipm-pack init
  aipm make plugin <args>  -- atomic scaffolding actions
  aipm lint --fix          -- auto-fix via make actions
  aipm make extension      -- future: engine-specific SDK integrations

Depends on:
  libaipm     -- shared library with new `make` module
    make::{Action, ActionRegistry, execute}  -- composable atomic primitives
    lint::rule::Fix (or IsFixable trait)     -- lint fix actions
```

### Scaffolding Data Flow (Current)

```
aipm-pack init [dir]
  └─→ wizard_tty::resolve()              [aipm-pack/wizard_tty.rs]
        └─→ libaipm::init::init()        [init.rs:57]
              ├─→ validate name           [init.rs:77 -- independent copy]
              ├─→ create_directory_layout  [init.rs:92]
              └─→ write aipm.toml         [init.rs:96 -- format! approach]

aipm init [dir]
  └─→ wizard_tty::resolve()              [aipm/wizard_tty.rs]
        └─→ libaipm::workspace_init::init()  [workspace_init/mod.rs:96]
              ├─→ init_workspace()         → write aipm.toml (workspace manifest)
              ├─→ scaffold_marketplace()   → create .ai/ tree, write marketplace.json,
              │                               plugin.json, SKILL.md, hooks.json, etc.
              └─→ adaptors::claude::apply() → merge .claude/settings.json
```

---

## Historical Context (from research/)

### Directly Relevant Prior Research

- [`research/docs/2026-04-12-dry-rust-architecture-audit.md`](https://github.com/TheLarkInn/aipm/blob/06bc32f/research/docs/2026-04-12-dry-rust-architecture-audit.md) -- **Primary prior work.** Comprehensive DRY audit scoped as pre-work for #363. Identifies 10 deduplication targets. Covers #363, #361, #356. Sections E.1 and E.2 directly analyze #363 and #361 dependencies. **No coverage of #417** (this is the first research on that issue).

- [`research/progress.txt`](https://github.com/TheLarkInn/aipm/blob/06bc32f/research/progress.txt) -- Documents completed phases of the DRY consolidation: manifest builder unification, plugin.json generation consolidation, marketplace.json and settings.json module extraction, wizard type sharing.

- [`research/docs/2026-03-16-rust-cross-platform-release-distribution.md`](https://github.com/TheLarkInn/aipm/blob/06bc32f/research/docs/2026-03-16-rust-cross-platform-release-distribution.md) -- Documents how cargo-dist handles two binary targets as independent release artifacts.

- [`research/docs/2026-03-19-cargo-dist-installer-github-releases.md`](https://github.com/TheLarkInn/aipm/blob/06bc32f/research/docs/2026-03-19-cargo-dist-installer-github-releases.md) -- Contains section "Workspace Behavior with Two Binaries" relevant to merge impact.

- [`research/docs/2026-04-06-feature-status-audit.md`](https://github.com/TheLarkInn/aipm/blob/06bc32f/research/docs/2026-04-06-feature-status-audit.md) -- Comprehensive audit of all CLI command statuses.

- [`research/docs/2026-03-31-110-aipm-lint-architecture-research.md`](https://github.com/TheLarkInn/aipm/blob/06bc32f/research/docs/2026-03-31-110-aipm-lint-architecture-research.md) -- Notes "No `--fix` auto-fix infrastructure" as a gap, relevant to #363's lint --fix requirement.

- [`research/tickets/2026-03-28-110-aipm-lint.md`](https://github.com/TheLarkInn/aipm/blob/06bc32f/research/tickets/2026-03-28-110-aipm-lint.md) -- Documents planned `--fix` auto-fix mode with no existing infrastructure.

### Related Issues

- **#361** ([cli] can starter plugin just use 'aipm' instead of random typescript?) -- Directly depends on #363. Wants to replace the TypeScript scaffold script (`workspace_init/mod.rs:342`) with `aipm make` CLI calls.
- **#356** (Starter plugin fails default aipm lint checks) -- The `plugin.json` generated during init lacks component fields. Root cause is the DRY violation B2 (two independent plugin.json generators). Already addressed by the `generate::plugin_json` consolidation.

---

## Related Research

- `research/docs/2026-04-12-dry-rust-architecture-audit.md` -- Primary prerequisite analysis
- `research/docs/2026-04-06-plugin-system-feature-parity-analysis.md` -- Plugin lifecycle capabilities
- `research/docs/2026-03-20-scaffold-plugin-ts-missing-features.md` -- TypeScript scaffold gaps
- `research/docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md` -- All 5 manifest generation paths
- `research/docs/2026-03-19-init-tool-adaptor-refactor.md` -- ToolAdaptor trait design

---

## Open Questions

1. **Subcommand naming for merged init:** Should it be `aipm pack init`, `aipm init --plugin`, or something else? If `aipm make plugin new` is the long-term replacement, does it matter?

2. **aipm-pack crates.io deprecation:** If `aipm-pack` is removed from the workspace, should the existing crates.io package be yanked or left with a deprecation notice pointing to `aipm`?

3. **DRY consolidation completeness:** The `progress.txt` shows phases completed but the current codebase state needs verification against the full audit. Which of the 10 targets remain unfinished?

4. **lint --fix scope for #363:** Issue #363 mentions `aipm lint --fix` but the `Rule` trait has no fix capability. How much of the lint extension belongs to #363 vs a separate issue?

5. **`aipm make extension <--engine>` scope:** Issue #363 mentions engine-specific SDK integrations as future work. Should this be tracked separately to keep #363 focused?
