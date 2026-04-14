---
date: 2026-04-14 14:34:34 UTC
researcher: Claude (Opus 4.6)
git_commit: 2b94a6c3daa92851046553a429fc3a3d703a5541
branch: main
repository: aipm
topic: "Merge aipm-pack into aipm (#417) — post-#363 analysis"
tags: [research, codebase, aipm-pack, aipm-make, cli-consolidation, scaffolding, binary-merge]
status: complete
last_updated: 2026-04-14
last_updated_by: Claude (Opus 4.6)
last_updated_note: "Complete rewrite reflecting post-#363 state — aipm make plugin is now implemented, making aipm-pack init functionally obsolete"
---

# Research: Merge aipm-pack into aipm (#417) — Post-#363 Analysis

## Research Question

Document the current state of `aipm-pack` and `aipm` CLIs — their command surfaces, shared dependencies, structural overlap, and what would be involved in merging `aipm-pack`'s functionality into `aipm`, now that #363's `aipm make plugin` scaffolding replaces `aipm-pack init`.

## Summary

**Issue #363 is now closed and fully implemented.** The `aipm make plugin` command exists in the `aipm` CLI at commit `2b94a6c` with engine targeting, feature-based composition, marketplace integration, and an interactive wizard. This makes `aipm-pack init` functionally obsolete — confirming the prediction in #417's description.

`aipm-pack` is a thin 4-file CLI binary (~635 lines of source) that exposes a single `init` subcommand. Every dependency it uses is already present in `aipm`. `aipm make plugin` is a strict superset of `aipm-pack init`'s capability: it creates plugins inside a marketplace, registers them in `marketplace.json`, enables them in engine settings, and supports engine-specific feature filtering — none of which `aipm-pack init` does.

The only unique capability `aipm-pack init` retains is generating a standalone `aipm.toml` plugin manifest (TOML format), while `aipm make plugin` generates `plugin.json` (JSON format) inside a marketplace. If standalone `aipm.toml` package scaffolding is still desired, it can be added as `aipm make package` or similar — but the core use case (creating plugins in a marketplace) is fully covered by `aipm make plugin`.

The merge is now a deletion task with targeted test migration, not a code migration. The mechanical changes touch ~15 areas of the codebase (source, tests, BDD features, CI, docs, release config).

---

## Detailed Findings

### A. Current Command Surfaces

#### aipm CLI (12 commands at `2b94a6c`)

| Command | Purpose | Handler |
|---|---|---|
| `init` | Initialize workspace + marketplace | [`main.rs:361-408`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/aipm/src/main.rs#L361) |
| `install` | Install packages from registry/git | [`main.rs:410-446`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/aipm/src/main.rs#L410) |
| `update` | Update packages to latest compatible | [`main.rs:448-474`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/aipm/src/main.rs#L448) |
| `link` | Link local package for development | [`main.rs:476-517`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/aipm/src/main.rs#L476) |
| `uninstall` | Uninstall or unlink a package | [`main.rs:519-537`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/aipm/src/main.rs#L519) |
| `unlink` | Unlink a previously linked package | via `cmd_unlink` |
| `list` | List installed packages / links | [`main.rs:539-579`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/aipm/src/main.rs#L539) |
| `lint` | Lint AI plugin configurations | [`main.rs:680-760`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/aipm/src/main.rs#L680) |
| `migrate` | Migrate legacy tool configs into marketplace | [`main.rs:861-964`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/aipm/src/main.rs#L861) |
| **`make plugin`** | **Scaffold new plugin in marketplace** | [**`main.rs:966-1061`**](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/aipm/src/main.rs#L966) |
| `lsp` | Start Language Server Protocol server | [`main.rs:1134`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/aipm/src/main.rs#L1134) |
| `install --global` | Install globally for all projects | [`main.rs:581-610`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/aipm/src/main.rs#L581) |

#### aipm-pack CLI (1 command)

| Command | Purpose | Handler |
|---|---|---|
| `init` | Initialize a standalone plugin package | [`main.rs:46-75`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/aipm-pack/src/main.rs#L46) |

Planned but never implemented: `pack`, `publish`, `yank`, `login`.

### B. Functional Overlap: `aipm make plugin` vs `aipm-pack init`

Now that #363 is implemented, here is the precise feature comparison:

| Capability | `aipm-pack init` | `aipm make plugin` |
|---|---|---|
| Creates plugin directory | Yes (standalone) | Yes (inside marketplace) |
| Plugin name validation | Yes (`manifest::validate`) | Yes (`manifest::validate`) |
| Interactive wizard | Yes (inquire-based) | Yes (inquire-based, two-phase) |
| Non-interactive mode | `--yes` flag | `--yes` flag + `--name`/`--feature` |
| Manifest format | `aipm.toml` (TOML) | `plugin.json` (JSON) |
| Plugin type selection | 6 types (Composite, Skill, Agent, MCP, Hook, LSP) | 7 features (Skill, Agent, MCP, Hook, OutputStyle, LSP, Extension) |
| Engine targeting | None | `--engine claude/copilot/both` |
| Feature-based composition | Single type per plugin | Multiple features per plugin |
| Marketplace registration | None | Yes (`marketplace.json`) |
| Engine settings integration | None | Yes (`.claude/settings.json`) |
| Marketplace discovery | None | Walk-up search for `.ai/` |
| Structured action log | None (`Result<(), Error>`) | Yes (`Vec<Action>`) |
| `.gitkeep` placeholders | In every directory | Only for Extension feature |
| `Composite` type | Yes (creates skills+agents+hooks) | No composite type; multi-feature selection instead |
| Description prompt | Yes | No |
| Idempotency | Error if exists | Graceful `AlreadyExists` action |

**Key takeaway:** `aipm make plugin` is a strict superset in terms of integration capabilities. The only thing `aipm-pack init` produces that `make` doesn't is a standalone `aipm.toml` manifest — but that format is only needed for publishing to a registry (not yet implemented), while `plugin.json` is what the marketplace and engine settings actually consume.

### C. Library Module Comparison: `libaipm::init` vs `libaipm::make`

**`libaipm::init`** ([`init.rs`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/libaipm/src/init.rs)):
- Single file, 644 lines (including tests)
- Entry: `pub fn init(opts: &Options, fs: &dyn Fs) -> Result<(), Error>`
- Produces: `aipm.toml` via `manifest::builder::build_plugin_manifest()`
- Creates directory trees per `PluginType` enum (6 variants)
- Has no awareness of marketplace, engine settings, or `plugin.json`
- Only consumer: `aipm-pack init`

**`libaipm::make`** ([`make/`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/libaipm/src/make/) directory, 6 files):
- `mod.rs` (350+ lines) — orchestrator with `pub fn plugin()` and feature scaffolders
- `action.rs` — 9-variant `Action` enum for structured reporting
- `discovery.rs` — marketplace walk-up search
- `engine_features.rs` — engine/feature mapping, validation, `Feature` enum (7 variants)
- `error.rs` — 8-variant error enum
- `templates.rs` — content templates per feature type
- Entry: `pub fn plugin(opts: &PluginOpts, fs: &dyn Fs) -> Result<PluginResult, Error>`
- Produces: `plugin.json`, marketplace registration, engine settings integration
- Only consumer: `aipm make plugin`

**Shared internal utilities:** None. The two modules share zero imports between each other. Both use `crate::fs::Fs` and `crate::manifest::validate` but through independent call paths.

### D. Dependency Overlap

Every `aipm-pack` dependency is already in `aipm`:

| Dependency | aipm-pack | aipm | Unique to aipm |
|---|---|---|---|
| `libaipm` (wizard) | Yes | Yes | |
| `clap` | Yes | Yes | |
| `inquire` | Yes | Yes | |
| `serde` | Yes | Yes | |
| `serde_json` | Yes | Yes | |
| `thiserror` | Yes | Yes | |
| `toml` | Yes | Yes | |
| `tracing` | Yes | Yes | |
| `tracing-subscriber` | Yes | Yes | |
| `clap-verbosity-flag` | | | Yes |
| `tokio` | | | Yes |
| `tower-lsp` | | | Yes |

Merging adds zero new dependencies to the `aipm` binary.

### E. Wizard Overlap (Detailed)

Both crates implement a two-layer wizard architecture:
- **Pure logic layer** (`wizard.rs`): Defines prompt steps, resolves answers
- **TTY bridge layer** (`wizard_tty.rs`): Executes prompts via `inquire`

#### Fully Duplicated: `execute_prompts()`

The `execute_prompts` function exists identically in both TTY modules:
- [`crates/aipm-pack/src/wizard_tty.rs:46-114`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/aipm-pack/src/wizard_tty.rs#L46)
- [`crates/aipm/src/wizard_tty.rs:176-244`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/aipm/src/wizard_tty.rs#L176)

Same signature, same dispatch logic over 4 `PromptKind` variants, same validation wiring, same MultiSelect index resolution. Only cosmetic difference: match arm ordering.

#### Shared Patterns
- Both re-export `libaipm::wizard::{PromptStep, PromptKind, PromptAnswer, styled_render_config}`
- Both use `inquire::set_global_render_config(styled_render_config())` at interactive flow start
- Both follow the same interactive/non-interactive dispatch pattern
- Both use sequential `idx` counter for answer resolution
- Both define identical `format_steps` test helpers for snapshot tests
- Both define identical `validate_name_interactive` test helpers

#### aipm-pack Wizard Functions (Unique — no counterpart in aipm)
- `package_prompt_steps(dir, flag_name, flag_type)` — builds name/description/type prompts
- `resolve_package_answers(answers, flag_name, flag_type)` — resolves `(Option<String>, Option<PluginType>)`
- `plugin_type_from_index(index)` — maps select index to `PluginType` enum
- `PLUGIN_TYPE_OPTIONS` — 6-element array of human-readable plugin type labels
- Description prompt (always-shown text prompt)

#### aipm Wizard Functions (Unique — no counterpart in aipm-pack)
- `workspace_prompt_steps` / `resolve_workspace_answers` / `resolve_defaults` — workspace init
- `migrate_cleanup_prompt_steps` / `resolve_migrate_cleanup_answer` — post-migration
- `resolve_make_plugin` (in `wizard_tty.rs`) — two-phase make-plugin wizard with engine-filtered features
- `ENGINE_OPTIONS` / `engine_from_index` — engine mapping for make plugin
- `make_plugin_prompt_steps` / `resolve_make_plugin_answers` — `#[cfg(test)]` only

### F. Complete Blast Radius of Removing aipm-pack

#### Mandatory Changes (build/test breakage)

| File | Lines | Change Required |
|---|---|---|
| [`Cargo.toml`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/Cargo.toml#L177) | 177-178 | Remove `[profile.dev.package.aipm-pack]` section |
| `crates/aipm-pack/` | entire dir | Delete (4 source files, 11 snapshots, 1 E2E test file, 1 changelog, 1 Cargo.toml) |
| [`crates/libaipm/tests/bdd.rs`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/libaipm/tests/bdd.rs#L108) | 108-109 | Update binary name routing for `aipm-pack init` scenarios |
| [`tests/features/manifest/init.feature`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/tests/features/manifest/init.feature) | 9,16,21,26,36,50 | Update `aipm-pack init` command strings (6 occurrences) |
| `Cargo.lock` | auto | Regenerated automatically |

#### Documentation Updates (accuracy)

| File | Occurrences | Description |
|---|---|---|
| [`README.md`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/README.md) | 7+ lines | Module table, aipm-pack CLI section, project structure, roadmap |
| [`CLAUDE.md`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/CLAUDE.md#L88) | 1 line | Project structure entry for `crates/aipm-pack/` |
| [`docs/guides/creating-a-plugin.md`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/docs/guides/creating-a-plugin.md) | 9 lines | All references to `aipm-pack init` as scaffold command |
| [`docs/guides/make-plugin.md`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/docs/guides/make-plugin.md) | 3 lines | Comparison table and see-also links |
| [`docs/guides/init.md`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/docs/guides/init.md) | 2 lines | Cross-references to `aipm-pack init` |
| [`docs/README.md`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/docs/README.md) | 3 lines | References to aipm-pack |

#### Library Doc Comments

| File | Line | Comment to Update |
|---|---|---|
| [`crates/libaipm/src/lib.rs`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/libaipm/src/lib.rs#L4) | 4 | `"and the 'aipm-pack' author binary"` |
| [`crates/libaipm/src/init.rs`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/libaipm/src/init.rs#L1) | 1 | `"Package initialization and scaffolding for 'aipm-pack init'."` |
| [`crates/libaipm/src/wizard.rs`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/libaipm/src/wizard.rs#L4) | 4 | `"Both the 'aipm' and 'aipm-pack' binaries enable this feature"` |

#### CI/CD

| File | Lines | Change |
|---|---|---|
| [`.github/workflows/update-latest-release.yml`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/.github/workflows/update-latest-release.yml#L19) | 19-20 | Simplify tag filter (remove `aipm-pack-v` exclusion) |
| [`.github/workflows/docs-updater.md`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/.github/workflows/docs-updater.md#L101) | 101 | Remove `crates/aipm-pack/src/main.rs` reference |
| `dist-workspace.toml` | auto | cargo-dist stops discovering `aipm-pack` binary automatically |

#### Aspirational BDD Features (7 files, ~55 lines)

These reference future `aipm-pack` commands that were never implemented:
- `tests/features/registry/publish.feature` — `aipm-pack pack`, `aipm-pack publish`
- `tests/features/registry/yank.feature` — `aipm-pack yank`, `aipm-pack deprecate`
- `tests/features/registry/security.feature` — `aipm-pack publish`, `aipm-pack login`
- `tests/features/registry/link.feature` — mentions `aipm-pack` in description
- `tests/features/guardrails/quality.feature` — `aipm-pack init`, `aipm-pack lint`, `aipm-pack publish`
- `tests/features/monorepo/orchestration.feature` — `aipm-pack lint`, `aipm-pack publish`
- `tests/features/reuse/compositional-reuse.feature` — `aipm-pack publish`

These should be updated to use `aipm` subcommands (e.g., `aipm pack`, `aipm publish`, `aipm make plugin`) if/when those features are implemented.

### G. Decision: What Happens to `libaipm::init`?

With `aipm-pack` deleted, `libaipm::init` loses its only consumer. There are two paths:

**Option A: Delete `libaipm::init` entirely.**
- `aipm make plugin` fully covers the marketplace-based workflow.
- If standalone package scaffolding is ever needed, `aipm make package` can be added to the `make` module using the same action/template pattern.
- Simplest approach — removes ~644 lines and the `manifest::builder::build_plugin_manifest()` call path.

**Option B: Keep `libaipm::init` and wire it into `aipm` under `aipm pack init`.**
- Preserves the `aipm.toml`-based standalone package workflow.
- Needed if users want to create packages for registry publishing (future `aipm publish`).
- Requires adding `Init(libaipm::init::Error)` to `aipm`'s `CliError` and moving the wizard.

The choice depends on whether standalone `aipm.toml` packages are part of the near-term roadmap or not.

### H. E2E Test Migration

`crates/aipm-pack/tests/init_e2e.rs` contains 17 tests exercising the `aipm-pack` binary:

| Test | What it exercises |
|---|---|
| `init_default_creates_manifest_and_skill_dir` | Default init creates aipm.toml + skills/ |
| `init_custom_name_and_type` | `--name` and `--type` flags |
| `init_yes_flag_skips_prompts` | Non-interactive mode |
| `init_fails_if_already_initialized` | Idempotency error |
| `init_invalid_name_rejected` | Name validation |
| `init_each_plugin_type_*` (6 tests) | Each of the 6 plugin types |
| `init_in_subdirectory` | Positional dir argument |
| `init_default_name_from_dir` | Name derivation from directory |
| `help_shows_usage` / `version_shows_version` | Help/version output |

All 17 use `Command::cargo_bin("aipm-pack")`. Migration path depends on decision G above:
- If Option A (delete init): these tests are removed entirely (their coverage is subsumed by `aipm make plugin` tests).
- If Option B (keep init): retarget to `cargo_bin("aipm")` with `["pack", "init", ...]` arguments.

---

## Architecture Documentation

### Current Two-Binary Architecture (at `2b94a6c`)

```
User installs:
  aipm        -- unified CLI (init, install, update, link, unlink, list, lint, migrate, make, lsp)
  aipm-pack   -- author CLI (init only, functionally obsolete)

Both depend on:
  libaipm     -- shared library (30+ public modules)
    features = ["wizard"] gates inquire-dependent shared types

Binary output:
  cargo-dist builds 2 binaries x 4 platforms = 8 artifacts per release
  release-plz publishes 3 crates to crates.io (aipm, aipm-pack, libaipm)
```

### Post-Merge Architecture (#417)

```
User installs:
  aipm        -- single CLI (init, install, update, link, unlink, list, lint, migrate, make, lsp)

Depends on:
  libaipm     -- shared library (unchanged or with init module removed)

Binary output:
  cargo-dist builds 1 binary x 4 platforms = 4 artifacts per release
  release-plz publishes 2 crates to crates.io (aipm, libaipm)
```

### Scaffolding Data Flow (Post-#363, Current State)

```
aipm-pack init [dir]                              ← OBSOLETE
  └→ wizard_tty::resolve()                        [aipm-pack/wizard_tty.rs]
       └→ libaipm::init::init()                   [init.rs:57]
             ├→ validate name                     [init.rs:77]
             ├→ create_directory_layout            [init.rs:92]
             └→ write aipm.toml                   [init.rs:96 — standalone TOML manifest]

aipm make plugin --name X --engine claude --feature skill
  └→ wizard_tty::resolve_make_plugin()            [aipm/wizard_tty.rs:78]
       ├→ Phase 1: name + engine prompts          [wizard_tty.rs:98-135]
       ├→ engine_features::validate_features()    [engine_features.rs:108]
       ├→ Phase 2: feature multi-select prompt    [wizard_tty.rs:144-170]
       └→ libaipm::make::plugin()                 [make/mod.rs:55]
             ├→ create plugin directory            [mod.rs:60]
             ├→ scaffold per-feature artifacts     [mod.rs:130-258]
             ├→ generate plugin.json               [mod.rs:262-283]
             ├→ register in marketplace.json       [mod.rs:287-306]
             └→ enable in .claude/settings.json    [mod.rs:309-333]
```

---

## Historical Context (from research/)

### Directly Relevant Prior Research

- [`research/docs/2026-04-12-dry-rust-architecture-audit.md`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/research/docs/2026-04-12-dry-rust-architecture-audit.md) — DRY audit identifying 10 deduplication targets including wizard consolidation (B7) and duplicate `execute_prompts` functions.

- [`research/tickets/2026-04-14-0363-aipm-make-foundational-api.md`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/research/tickets/2026-04-14-0363-aipm-make-foundational-api.md) — Implementation research for the now-completed #363.

- [`research/feature-list.json`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/research/feature-list.json) — 22-phase implementation plan for `aipm make plugin` (#363), all phases completed.

- [`research/progress.txt`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/research/progress.txt) — Execution log for all 22 phases of #363, documenting completion.

- [`research/docs/2026-04-06-feature-status-audit.md`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/research/docs/2026-04-06-feature-status-audit.md) — Notes `aipm-pack pack/publish/yank/login` as NOT IMPLEMENTED.

- [`research/docs/2026-03-16-rust-cross-platform-release-distribution.md`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/research/docs/2026-03-16-rust-cross-platform-release-distribution.md) — Documents cargo-dist handling of two binary targets.

- [`research/docs/2026-03-09-cargo-core-principles.md`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/research/docs/2026-03-09-cargo-core-principles.md) — Cargo's single-binary-with-subcommands architectural model (precedent for consolidation).

### Design Specs Referencing aipm-pack

The following specs contain substantive references to `aipm-pack` (these are historical design documents and do not need mechanical changes):

| Spec | Ref Count | Context |
|---|---|---|
| `specs/2026-03-22-interactive-init-wizard.md` | ~30 | Full wizard design for aipm-pack init |
| `specs/2026-03-09-aipm-technical-design.md` | ~20 | Original two-binary architecture rationale |
| `specs/2026-03-16-ci-cd-release-automation.md` | ~20 | Release pipeline for both binaries |
| `specs/2026-03-19-cargo-dist-installers.md` | ~15 | Installer scripts and archive names |
| `specs/2026-04-14-aipm-make-plugin-command.md` | ~2 | Notes aipm-pack integration |

---

## Related Research

- `research/docs/2026-04-12-dry-rust-architecture-audit.md` — Primary prerequisite analysis (wizard dedup target B7)
- `research/docs/2026-04-06-plugin-system-feature-parity-analysis.md` — Plugin lifecycle capabilities
- `research/docs/2026-03-20-scaffold-plugin-ts-missing-features.md` — TypeScript scaffold gaps (motivating #361)
- `research/docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md` — All manifest generation paths
- `research/docs/2026-03-19-init-tool-adaptor-refactor.md` — ToolAdaptor trait design

---

## Open Questions

1. **Delete vs. absorb `libaipm::init`?** With `aipm-pack` gone, should `libaipm::init` be deleted entirely (since `aipm make plugin` covers the use case), or kept and wired into `aipm` as `aipm pack init` for standalone package scaffolding? The answer depends on whether `aipm.toml`-based standalone packages are part of the near-term roadmap.

2. **Crates.io deprecation:** The `aipm-pack` package on crates.io should be deprecated with a notice pointing to `aipm`. Should it be yanked or left as deprecated?

3. **Wizard deduplication:** The `execute_prompts()` function is duplicated between both crates. Removing aipm-pack eliminates one copy, but the function remains in `aipm`'s `wizard_tty.rs`. The DRY audit (B7) recommends extracting it to `libaipm::wizard` — should this be done as part of #417 or separately?

4. **BDD feature migration:** The 6 BDD scenarios in `tests/features/manifest/init.feature` test `aipm-pack init`. Should they be rewritten to test `aipm make plugin` (the functional replacement), or dropped entirely if `aipm make plugin` already has sufficient test coverage?

5. **Aspirational BDD features:** 7 feature files reference never-implemented `aipm-pack` commands (`pack`, `publish`, `yank`, `login`). These need command name updates to `aipm` subcommands when those features are eventually built.
