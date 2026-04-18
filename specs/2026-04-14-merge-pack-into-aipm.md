# [pack] Merge aipm-pack into aipm — Technical Design Document

| Document Metadata      | Details                                                    |
| ---------------------- | ---------------------------------------------------------- |
| Author(s)              | Sean Larkin                                                |
| Status                 | Implemented                                                |
| Team / Owner           | aipm                                                       |
| Created / Last Updated | 2026-04-14                                                 |
| Issue                  | [#417](https://github.com/TheLarkInn/aipm/issues/417)      |
| Depends on             | [#363](https://github.com/TheLarkInn/aipm/issues/363) (done) |

## 1. Executive Summary

This spec describes merging the `aipm-pack` author CLI binary into the `aipm` consumer CLI, eliminating the second binary from the workspace. With `aipm make plugin` (#363) now fully implemented, `aipm-pack init` is functionally obsolete for marketplace-based workflows. However, standalone `aipm.toml` package scaffolding remains valuable for future registry publishing, so `libaipm::init` is preserved and wired into `aipm` as `aipm pack init`. The merge also extracts the duplicated `execute_prompts()` wizard function into `libaipm::wizard`, completing DRY audit item B7. The result is a single CLI binary, simplified release pipeline (4 artifacts instead of 8), and zero duplicated wizard code.

## 2. Context and Motivation

### 2.1 Current State

The workspace produces two binary crates:

```
crates/
├── aipm/          → "aipm" binary (consumer CLI: init, install, update, link, unlink,
│                     list, lint, migrate, make plugin, lsp)
├── aipm-pack/     → "aipm-pack" binary (author CLI: init only)
└── libaipm/       → shared library (30+ public modules)
```

Both binaries depend on `libaipm` with the `wizard` feature. Both contain independent copies of the `execute_prompts()` TTY bridge function (~69 lines each). Both are built by cargo-dist for 4 platform targets (8 release artifacts total). Both are published to crates.io as separate packages.

**Key references:**
- Research: [`research/tickets/2026-04-14-0417-merge-pack-into-aipm.md`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/research/tickets/2026-04-14-0417-merge-pack-into-aipm.md)
- DRY audit: [`research/docs/2026-04-12-dry-rust-architecture-audit.md`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/research/docs/2026-04-12-dry-rust-architecture-audit.md) (item B7: wizard dedup)

### 2.2 The Problem

- **Two binaries for one tool.** Users must install and learn two separate CLIs (`aipm` and `aipm-pack`). The `aipm-pack` description says "author CLI (init, pack, publish, yank, login)" but only `init` is implemented — the other 4 commands were never built.
- **`aipm make plugin` supersedes `aipm-pack init`.** Since #363 landed, `aipm make plugin` creates plugins inside a marketplace with engine targeting, feature-based composition, `marketplace.json` registration, and `.claude/settings.json` integration — none of which `aipm-pack init` supports. The only unique capability of `aipm-pack init` is generating a standalone `aipm.toml` manifest.
- **Duplicated wizard infrastructure.** The `execute_prompts()` function is copy-pasted between both binaries ([`aipm-pack/wizard_tty.rs:46-114`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/aipm-pack/src/wizard_tty.rs#L46) and [`aipm/wizard_tty.rs:176-244`](https://github.com/TheLarkInn/aipm/blob/2b94a6c/crates/aipm/src/wizard_tty.rs#L176)). DRY audit B7 identified this.
- **Double release overhead.** cargo-dist builds 2 binaries × 4 platforms = 8 artifacts. release-plz manages 3 crates.io packages. The `update-latest-release.yml` workflow has a [guard to exclude `aipm-pack-v` tags](https://github.com/TheLarkInn/aipm/blob/2b94a6c/.github/workflows/update-latest-release.yml#L20).

### 2.3 Why Now

Issue #417 explicitly states: *"With the 'aipm make' api also on backlog, this will make 'aipm-pack init' obsolete."* That condition is now met — #363 is closed and merged. The merge is unblocked.

## 3. Goals and Non-Goals

### 3.1 Functional Goals

- [x] `aipm pack init` works identically to today's `aipm-pack init` (same flags, same output, same `libaipm::init` call path)
- [x] `crates/aipm-pack/` directory is deleted from the workspace
- [x] `execute_prompts()` is extracted into `libaipm::wizard` behind the `wizard` feature flag (DRY item B7)
- [x] All 21 E2E tests from `aipm-pack/tests/init_e2e.rs` are migrated to `aipm/tests/pack_init_e2e.rs`
- [x] BDD scenarios in `tests/features/manifest/init.feature` are rewritten to test `aipm make plugin`
- [x] Aspirational BDD features (7 files) are updated to use `aipm` subcommand names instead of `aipm-pack`
- [x] All documentation references to `aipm-pack` are updated
- [x] Release pipeline produces 1 binary × 4 platforms = 4 artifacts
- [x] `cargo build --workspace`, `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, `cargo fmt --check` all pass with zero warnings
- [x] 89% branch coverage gate is maintained

### 3.2 Non-Goals (Out of Scope)

- [ ] We will NOT implement `aipm pack`, `aipm publish`, `aipm yank`, or `aipm login` — those are future work
- [ ] We will NOT deprecate or yank the `aipm-pack` crate on crates.io as part of this change (separate ops task)
- [ ] We will NOT refactor `libaipm::init` internals — it is preserved as-is and wired into the new subcommand
- [ ] We will NOT modify `aipm make plugin` behavior
- [ ] We will NOT consolidate other DRY audit items (B1-B6, B8-B10) — only B7 (`execute_prompts`)
- [ ] We will NOT update historical spec documents — they are frozen design records

## 4. Proposed Solution (High-Level Design)

### 4.1 Post-Merge Architecture

```
crates/
├── aipm/          → single "aipm" binary (init, install, update, link, unlink,
│                     list, lint, migrate, make plugin, pack init, lsp)
└── libaipm/       → shared library with expanded wizard module
                      wizard.rs now contains execute_prompts()
```

### 4.2 Command Surface After Merge

```
aipm
├── init                  # Initialize workspace + marketplace (unchanged)
├── install               # Install packages (unchanged)
├── update                # Update packages (unchanged)
├── link                  # Link local package (unchanged)
├── uninstall             # Uninstall/unlink (unchanged)
├── unlink                # Unlink package (unchanged)
├── list                  # List packages/links (unchanged)
├── lint                  # Lint configurations (unchanged)
├── migrate               # Migrate legacy configs (unchanged)
├── make
│   └── plugin            # Scaffold plugin in marketplace (unchanged, from #363)
├── pack
│   └── init              # Scaffold standalone plugin package (NEW — absorbed from aipm-pack)
└── lsp                   # Language Server Protocol (unchanged)
```

### 4.3 Data Flow for `aipm pack init`

```
aipm pack init [dir] [--yes] [--name NAME] [--type TYPE]
  └→ wizard_tty::resolve_pack_init()              [aipm/wizard_tty.rs — NEW]
       ├→ libaipm::wizard::execute_prompts()       [libaipm/wizard.rs — EXTRACTED]
       └→ wizard::resolve_package_answers()        [aipm/wizard.rs — MOVED from aipm-pack]
            └→ libaipm::init::init()               [libaipm/init.rs — UNCHANGED]
```

### 4.4 Key Components

| Component | Responsibility | Change Type |
|---|---|---|
| `crates/aipm/src/main.rs` | Add `Pack { Init { ... } }` subcommand + `cmd_pack_init()` handler | Modified |
| `crates/aipm/src/error.rs` | Add `Init(libaipm::init::Error)` variant | Modified |
| `crates/aipm/src/wizard.rs` | Absorb `package_prompt_steps`, `resolve_package_answers`, `plugin_type_from_index`, `PLUGIN_TYPE_OPTIONS` from aipm-pack | Modified |
| `crates/aipm/src/wizard_tty.rs` | Add `resolve_pack_init()` bridge; remove local `execute_prompts()` — now calls `libaipm::wizard::execute_prompts()` | Modified |
| `crates/libaipm/src/wizard.rs` | Add `execute_prompts()` function (extracted from both binaries) | Modified |
| `crates/aipm-pack/` | Entire directory deleted | Deleted |

## 5. Detailed Design

### 5.1 Phase 1: Extract `execute_prompts()` to `libaipm::wizard`

**Goal:** Eliminate the duplicated 69-line function by extracting it to the shared library. This must land first because both subsequent phases depend on it.

**File: `crates/libaipm/src/wizard.rs`**

Add the following function below the existing `styled_render_config()`:

```rust
/// Execute a sequence of prompt steps against a real terminal via `inquire`.
///
/// Each `PromptStep` is dispatched to the corresponding `inquire` prompt type.
/// Text prompts with `validate: true` use `manifest::validate::check_name()`
/// in `Interactive` mode. Returns one `PromptAnswer` per step in order.
pub fn execute_prompts(steps: &[PromptStep]) -> Result<Vec<PromptAnswer>, Box<dyn std::error::Error>> {
    // ... (move the body from crates/aipm/src/wizard_tty.rs:176-244 verbatim)
}
```

The function body is identical in both binaries (only match arm ordering differs). Use the `aipm` version as the canonical source.

**Required import additions to `wizard.rs`:**
- `use crate::manifest::validate::{self, ValidationMode};` — for text prompt validation
- `use inquire::{Confirm, MultiSelect, Select, Text};` — prompt types

These are already behind the `#[cfg(feature = "wizard")]` gate, and the `wizard` feature already gates `dep:inquire`, so no `Cargo.toml` changes are needed.

**File: `crates/aipm/src/wizard_tty.rs`**

Remove the local `execute_prompts()` function (lines 176-244). Replace all call sites with:

```rust
libaipm::wizard::execute_prompts(steps)
```

Call sites to update:
- `resolve()` (init wizard, ~line 41)
- `resolve_migrate_cleanup()` (~line 63)
- `resolve_make_plugin()` (~line 113 and ~line 156)

**File: `crates/aipm-pack/src/wizard_tty.rs`**

No changes — this file is deleted entirely in Phase 3.

**Testing:** All existing wizard tests continue to pass since the function signature and behavior are unchanged. Add unit tests in `libaipm/src/wizard.rs` for `execute_prompts()` covering each `PromptKind` variant. Since `inquire` requires a real TTY, these tests should verify the function compiles and the type signatures are correct; interactive behavior is covered by E2E tests.

### 5.2 Phase 2: Absorb aipm-pack's CLI into aipm

#### 5.2.1 Add `Pack` Subcommand to `crates/aipm/src/main.rs`

Add to the `Commands` enum (after `Make`):

```rust
/// Author commands for plugin packages.
Pack {
    #[command(subcommand)]
    subcommand: PackSubcommand,
},
```

Add a new enum:

```rust
#[derive(Subcommand)]
enum PackSubcommand {
    /// Initialize a new AI plugin package.
    Init {
        /// Skip interactive prompts, use all defaults.
        #[arg(short = 'y', long)]
        yes: bool,

        /// Package name (defaults to directory name).
        #[arg(long)]
        name: Option<String>,

        /// Plugin type: skill, agent, mcp, hook, lsp, composite.
        #[arg(long, rename_all = "kebab-case", value_name = "TYPE")]
        r#type: Option<String>,

        /// Directory to initialize (defaults to current directory).
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
}
```

This preserves the exact same flags and positional argument as `aipm-pack init`.

#### 5.2.2 Add `cmd_pack_init()` Handler

```rust
fn cmd_pack_init(
    yes: bool,
    name: Option<&str>,
    r#type: Option<&str>,
    dir: PathBuf,
) -> Result<(), error::CliError> {
    let plugin_type = r#type
        .map(str::parse::<libaipm::manifest::types::PluginType>)
        .transpose()?;

    let dir = resolve_dir(dir)?;
    let interactive = !yes && std::io::stdin().is_terminal();

    let (final_name, final_type) =
        wizard_tty::resolve_pack_init(interactive, &dir, name.map(String::from), plugin_type)?;

    let opts = libaipm::init::Options {
        dir: &dir,
        name: final_name.as_deref(),
        plugin_type: final_type,
    };

    libaipm::init::init(&opts, &libaipm::fs::Real)?;

    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "Initialized plugin package in {}", dir.display());
    Ok(())
}
```

#### 5.2.3 Add Match Arm in `run()`

In the `match cli.command { ... }` block, add:

```rust
Some(Commands::Pack { subcommand }) => match subcommand {
    PackSubcommand::Init { yes, name, r#type, dir } => {
        cmd_pack_init(yes, name.as_deref(), r#type.as_deref(), dir)
    },
},
```

#### 5.2.4 Add Error Variant to `crates/aipm/src/error.rs`

Add to `CliError`:

```rust
/// Package init errors.
#[error(transparent)]
Init(#[from] libaipm::init::Error),
```

#### 5.2.5 Move Wizard Functions to `crates/aipm/src/wizard.rs`

Move from `aipm-pack/src/wizard.rs` into `aipm/src/wizard.rs`:

| Function/Constant | Source Location | Purpose |
|---|---|---|
| `PLUGIN_TYPE_OPTIONS` | `aipm-pack/wizard.rs:18` | 6-element array of plugin type labels |
| `plugin_type_from_index()` | `aipm-pack/wizard.rs:28` | Maps select index → `PluginType` |
| `package_prompt_steps()` | `aipm-pack/wizard.rs:43` | Builds name/description/type prompts |
| `resolve_package_answers()` | `aipm-pack/wizard.rs:87` | Resolves `(Option<String>, Option<PluginType>)` |

Also move the associated snapshot tests and test helpers. Snapshot files will need regeneration since `insta` derives snapshot names from the module path (the prefix changes from `aipm_pack__wizard__tests__` to `aipm__wizard__tests__`).

#### 5.2.6 Add TTY Bridge in `crates/aipm/src/wizard_tty.rs`

Add a new public function:

```rust
pub fn resolve_pack_init(
    interactive: bool,
    dir: &Path,
    flag_name: Option<String>,
    flag_type: Option<PluginType>,
) -> Result<(Option<String>, Option<PluginType>), Box<dyn std::error::Error>> {
    if !interactive {
        return Ok((flag_name, flag_type));
    }
    inquire::set_global_render_config(wizard::styled_render_config());
    let steps = wizard::package_prompt_steps(dir, flag_name.as_deref(), flag_type);
    let answers = libaipm::wizard::execute_prompts(&steps)?;
    Ok(wizard::resolve_package_answers(&answers, flag_name.as_deref(), flag_type))
}
```

This follows the exact same pattern as the existing `resolve()`, `resolve_migrate_cleanup()`, and `resolve_make_plugin()` functions. It delegates to `libaipm::wizard::execute_prompts()` (extracted in Phase 1).

### 5.3 Phase 3: Delete `crates/aipm-pack/`

Delete the entire directory:

```
crates/aipm-pack/
├── Cargo.toml
├── CHANGELOG.md
├── src/
│   ├── main.rs
│   ├── error.rs
│   ├── wizard.rs
│   ├── wizard_tty.rs
│   └── snapshots/ (11 files)
└── tests/
    └── init_e2e.rs
```

### 5.4 Phase 4: Migrate E2E Tests

Move `crates/aipm-pack/tests/init_e2e.rs` → `crates/aipm/tests/pack_init_e2e.rs`.

**Changes in every test:**
1. Replace `Command::cargo_bin("aipm-pack")` with `Command::cargo_bin("aipm")`
2. Prepend `["pack", "init"]` before existing arguments
3. Update test function names from `init_*` to `pack_init_*` (optional but improves clarity)

**Example transform:**

Before:
```rust
fn aipm_pack() -> assert_cmd::Command {
    Command::cargo_bin("aipm-pack").expect("binary exists")
}

#[test]
fn init_in_empty_directory_creates_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    aipm_pack()
        .arg("init")
        .arg(tmp.path())
        .assert()
        .success();
    assert!(tmp.path().join("aipm.toml").exists());
}
```

After:
```rust
fn aipm() -> assert_cmd::Command {
    Command::cargo_bin("aipm").expect("binary exists")
}

#[test]
fn pack_init_in_empty_directory_creates_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    aipm()
        .arg("pack")
        .arg("init")
        .arg(tmp.path())
        .assert()
        .success();
    assert!(tmp.path().join("aipm.toml").exists());
}
```

All 21 tests follow this mechanical transformation. The test assertions themselves do not change — the `libaipm::init` code path is unchanged.

### 5.5 Phase 5: Rewrite BDD Features

#### 5.5.1 Rewrite `tests/features/manifest/init.feature`

The 6 scenarios currently test `aipm-pack init`. Rewrite them to test `aipm make plugin` instead.

The scenario structure changes because `aipm make plugin` operates inside a marketplace (requires `aipm init` first) and produces `plugin.json` instead of `aipm.toml`. Example mapping:

| Old Scenario | New Scenario |
|---|---|
| "Initialize a new plugin in an empty directory" | "Create a new plugin in an initialized marketplace" |
| "Initialize with a custom name" | "Create a plugin with --name flag" |
| "Reject initialization in existing directory" | "Creating a plugin in existing directory succeeds idempotently" (make is idempotent) |
| "Initialize creates standard directory layout" | "Plugin scaffold includes feature directories" |
| "Initialize with a specific plugin type" (outline) | "Plugin scaffold with specific features" (outline over feature types) |
| "Package name must follow naming conventions" | "Plugin name must follow naming conventions" |

Each scenario should:
1. Start with `Given an initialized marketplace` (calls `aipm init --marketplace -y`)
2. Run `aipm make plugin --name X --engine claude --feature Y -y`
3. Assert on the plugin directory structure, `plugin.json`, and marketplace registration

#### 5.5.2 Update BDD Binary Routing

In `crates/libaipm/tests/bdd.rs`, remove the special-case routing at lines 108-111:

```rust
// Remove this block:
if binary == "aipm-pack" && args.first() == Some(&"init") && working_dir.is_some() {
    cmd.args(args);
    cmd.arg(cwd.to_str().unwrap());
}
```

Since `aipm-pack` is no longer a binary target, this branch is dead code. All BDD scenarios will use the `aipm` binary with `current_dir`.

#### 5.5.3 Update Aspirational BDD Features

In each of the 7 aspirational feature files, mechanically rename `aipm-pack <cmd>` to `aipm <cmd>`:

| File | Find | Replace |
|---|---|---|
| `tests/features/registry/publish.feature` | `aipm-pack pack` | `aipm pack` |
| | `aipm-pack publish` | `aipm publish` |
| `tests/features/registry/yank.feature` | `aipm-pack yank` | `aipm yank` |
| | `aipm-pack deprecate` | `aipm deprecate` |
| `tests/features/registry/security.feature` | `aipm-pack publish` | `aipm publish` |
| | `aipm-pack login` | `aipm login` |
| `tests/features/registry/link.feature` | `aipm-pack` (description) | `aipm` |
| `tests/features/guardrails/quality.feature` | `aipm-pack init` | `aipm pack init` |
| | `aipm-pack lint` | `aipm lint` |
| | `aipm-pack publish` | `aipm publish` |
| | `aipm-pack lint --fix` | `aipm lint --fix` |
| `tests/features/monorepo/orchestration.feature` | `aipm-pack lint` | `aipm lint` |
| | `aipm-pack publish` | `aipm publish` |
| `tests/features/reuse/compositional-reuse.feature` | `aipm-pack publish` | `aipm publish` |

### 5.6 Phase 6: Update Documentation and Configuration

#### 5.6.1 Workspace Root

| File | Change |
|---|---|
| `Cargo.toml:177-178` | Remove `[profile.dev.package.aipm-pack]` section |
| `README.md` | Remove "aipm-pack" CLI section, update module table, project structure, roadmap |
| `CLAUDE.md:88` | Change `crates/aipm-pack/ — author CLI binary (init)` to `(deleted — merged into aipm as 'aipm pack init')` or remove the line |

#### 5.6.2 Library Doc Comments

| File:Line | Old | New |
|---|---|---|
| `crates/libaipm/src/lib.rs:4` | `"and the 'aipm-pack' author binary"` | `"(formerly also the 'aipm-pack' author binary, now merged)"` |
| `crates/libaipm/src/init.rs:1` | `"Package initialization and scaffolding for 'aipm-pack init'."` | `"Package initialization and scaffolding for 'aipm pack init'."` |
| `crates/libaipm/src/wizard.rs:4` | `"Both the 'aipm' and 'aipm-pack' binaries enable this feature"` | `"The 'aipm' binary enables this feature for interactive wizard flows."` |

#### 5.6.3 Documentation Guides

| File | Change |
|---|---|
| `docs/guides/creating-a-plugin.md` | Replace all `aipm-pack init` references with `aipm pack init` (9 occurrences) |
| `docs/guides/make-plugin.md` | Update comparison table and see-also links (3 occurrences) |
| `docs/guides/init.md` | Update cross-references (2 occurrences) |
| `docs/README.md` | Update references (3 occurrences) |

#### 5.6.4 CI/CD

| File | Change |
|---|---|
| `.github/workflows/update-latest-release.yml:20` | Remove `&& !startsWith(github.event.release.tag_name, 'aipm-pack-v')` guard |
| `.github/workflows/docs-updater.md:101` | Remove `crates/aipm-pack/src/main.rs` reference, then `gh aw compile docs-updater` |

No changes needed for:
- `ci.yml` — `--workspace` flags auto-adapt
- `release.yml` — cargo-dist auto-discovers from workspace
- `release-plz.toml` — auto-discovers workspace members
- `dist-workspace.toml` — auto-discovers dist-able binaries

## 6. Alternatives Considered

| Option | Pros | Cons | Decision |
|---|---|---|---|
| **A: Delete aipm-pack + delete libaipm::init** | Simplest; removes all dead code. `aipm make plugin` covers the use case. | Loses standalone `aipm.toml` package scaffolding needed for future `aipm publish`. | Rejected — standalone packages are part of the roadmap. |
| **B: Delete aipm-pack + wire init as `aipm pack init`** | Preserves standalone workflow. Future `pack`, `publish`, `yank`, `login` can be added under `aipm pack`. | Minor added scope vs. Option A. | **Selected.** |
| **C: Keep aipm-pack binary, just extract shared code** | No breaking change for existing users. | Doesn't achieve the single-binary goal. Users still need two installs. Release overhead remains. | Rejected — defeats the purpose of #417. |
| **D: Alias `aipm-pack` as a shell wrapper around `aipm pack`** | Backwards-compatible. | Adds complexity, still builds two artifacts (or requires a wrapper script in the install). | Rejected — unnecessary complexity. |

## 7. Cross-Cutting Concerns

### 7.1 Backwards Compatibility

Users who have `aipm-pack` installed will need to update. Since `aipm-pack` was never distributed via a stable installer (the `update-latest-release.yml` workflow [explicitly excludes `aipm-pack-v` tags](https://github.com/TheLarkInn/aipm/blob/2b94a6c/.github/workflows/update-latest-release.yml#L20)), the impact is minimal. The `aipm-pack` binary will no longer be produced in releases. Users should run `aipm pack init` instead.

### 7.2 Coverage

The 89% branch coverage gate must be maintained. The merge should be coverage-neutral or positive:
- `libaipm::init` retains its existing unit tests (they don't depend on the binary crate)
- `libaipm::wizard::execute_prompts()` gains new unit tests
- 21 E2E tests migrate to `aipm` crate (same coverage, different binary)
- BDD scenarios are rewritten (equivalent or broader coverage)

### 7.3 Crates.io

The `aipm-pack` package on crates.io is a separate ops concern (non-goal). It can be deprecated later with a notice pointing to `aipm pack init`.

## 8. Migration, Rollout, and Testing

### 8.1 Implementation Order

The phases must be executed in order due to dependencies:

```
Phase 1: Extract execute_prompts → libaipm::wizard
    ↓
Phase 2: Add Pack subcommand + wire aipm pack init
    ↓
Phase 3: Delete crates/aipm-pack/
    ↓
Phase 4: Migrate E2E tests → crates/aipm/tests/pack_init_e2e.rs
    ↓
Phase 5: Rewrite BDD features + update aspirational features
    ↓
Phase 6: Update docs, CI, configuration
```

Phases 3-6 can potentially be combined into fewer commits, but the logical ordering must be preserved. Phases 1 and 2 should be separate commits to keep changes reviewable.

### 8.2 Test Plan

**After Phase 1 (wizard extraction):**
- [x] `cargo test --workspace` passes (all existing tests use the extracted function)
- [x] `cargo clippy --workspace -- -D warnings` passes
- [x] New unit tests for `libaipm::wizard::execute_prompts()` pass

**After Phase 2 (pack init subcommand):**
- [x] `aipm pack init` in a temp directory produces `aipm.toml` and correct directory layout
- [x] `aipm pack init --name hello --type skill -y` works non-interactively
- [x] `aipm pack init` in a directory with existing `aipm.toml` fails with "already initialized"
- [x] `aipm pack init --name INVALID!` fails with validation error
- [x] All existing `aipm` commands continue to work unchanged

**After Phase 3 (delete aipm-pack):**
- [x] `cargo build --workspace` succeeds (no reference to deleted crate)
- [x] `cargo test --workspace` passes

**After Phase 4 (E2E migration):**
- [x] All 21 migrated E2E tests pass targeting `aipm pack init`
- [x] `cargo test --workspace` passes

**After Phase 5 (BDD rewrite):**
- [x] BDD test suite passes (cucumber-rs)
- [x] Aspirational features parse correctly (no syntax errors)

**After Phase 6 (docs/CI):**
- [x] `cargo build --workspace` still passes
- [x] `cargo clippy --workspace -- -D warnings` passes
- [x] `cargo fmt --check` passes
- [x] Coverage gate: `cargo +nightly llvm-cov report --doctests --branch --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs|lsp\.rs)'` shows ≥ 89%
- [x] `gh aw compile docs-updater` succeeds

### 8.3 Verification Checklist

- [x] `grep -r "aipm-pack" crates/` returns zero matches (excluding test snapshots and comments noting the merge)
- [x] `grep -r "aipm-pack" tests/features/` returns zero matches
- [x] `grep -r "aipm-pack" .github/workflows/` returns zero matches (or only in lock files pending recompilation)
- [x] `cargo dist plan` output shows only `aipm` binary, not `aipm-pack`
- [x] `aipm --help` shows `pack` subcommand
- [x] `aipm pack init --help` shows same flags as old `aipm-pack init --help`

## 9. Open Questions / Unresolved Issues

- [ ] **Crates.io deprecation timing:** When should the `aipm-pack` crate on crates.io be deprecated? Before or after this change ships? (Separate ops task, not blocking.)
- [ ] **Shell completion scripts:** If any shell completion scripts reference `aipm-pack`, they need updating. Verify whether cargo-dist or clap_complete generates these.
- [ ] **`PluginType::from_str` error handling:** The current `aipm-pack` uses `str::parse::<PluginType>()` which returns a `manifest::error::Error`. Verify this error type converts cleanly through `aipm`'s `CliError` (it should, via the existing `Manifest` variant).
