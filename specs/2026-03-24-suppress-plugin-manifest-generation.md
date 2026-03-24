# Suppress Plugin Manifest Generation

| Document Metadata      | Details                              |
| ---------------------- | ------------------------------------ |
| Author(s)              | selarkin                             |
| Status                 | Draft (WIP)                          |
| Team / Owner           | aipm                                 |
| Created / Last Updated | 2026-03-24                           |

## 1. Executive Summary

Today, `aipm init` and `aipm migrate` unconditionally generate `aipm.toml` plugin manifest files for every plugin they create or discover. Since the local marketplace linking and dependency management system is not yet implemented, these manifests create user confusion — they imply a dependency resolution system that does not exist. This spec flips the default: **plugin-level `aipm.toml` files are no longer generated unless the user passes `--manifest`**. Workspace root manifests (`[workspace]` section) and `aipm-pack init` manifests are unaffected. Plugin registration in `marketplace.json` continues to work without manifests, and a future `aipm manifest generate` command will retroactively create manifests when the dependency system ships.

## 2. Context and Motivation

### 2.1 Current State

There are five distinct `aipm.toml` generation paths across two binaries (`aipm` and `aipm-pack`). All use hardcoded `format!()` string templates in `libaipm` (see [research](../research/docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md)):

| # | Path | Binary | Output Location | Generator Function |
|---|------|--------|-----------------|-------------------|
| 1 | Workspace manifest | `aipm` | `{dir}/aipm.toml` | `workspace_init::generate_workspace_manifest()` |
| 2 | Starter plugin manifest | `aipm` | `{dir}/.ai/starter-aipm-plugin/aipm.toml` | `workspace_init::generate_starter_manifest()` |
| 3 | Package init manifest | `aipm-pack` | `{dir}/aipm.toml` | `init::generate_manifest()` |
| 4 | Single-artifact migrate | `aipm` | `{dir}/.ai/{name}/aipm.toml` | `migrate::emitter::generate_plugin_manifest()` |
| 5 | Package-scoped migrate | `aipm` | `{dir}/.ai/{name}/aipm.toml` | `migrate::emitter::generate_package_manifest()` |

Paths 2, 4, and 5 produce **plugin-level** `aipm.toml` files (with `[package]` sections). These are the ones that imply dependency management. Path 1 produces a **workspace root** manifest that is harmless structural config. Path 3 is the **author tool** where manifests are expected.

Claude Code's plugin discovery does not require `aipm.toml` — it reads `marketplace.json` and component files directly via `extraKnownMarketplaces` in `.claude/settings.json` (see [Claude Code defaults research](../research/docs/2026-03-16-claude-code-defaults.md)).

### 2.2 The Problem

- **User confusion**: Running `aipm init` or `aipm migrate` produces `aipm.toml` files with `[dependencies]`, `version`, and `edition` fields that suggest a package management system. Users attempt to add dependencies, set versions, or publish — none of which work yet.
- **Premature commitment**: Generated manifests lock in schema decisions (hardcoded `version = "0.1.0"`, `edition = "2024"`) before the dependency resolver is built. When the resolver ships, these manifests may need updating.
- **No functional purpose**: Plugin-level manifests serve no runtime purpose today. Claude Code ignores them entirely.

## 3. Goals and Non-Goals

### 3.1 Functional Goals

- [ ] `aipm migrate` does NOT generate `aipm.toml` by default
- [ ] `aipm init --marketplace` does NOT generate `aipm.toml` for the starter plugin by default
- [ ] Both commands accept `--manifest` flag to opt in to `aipm.toml` generation
- [ ] Plugin registration in `marketplace.json` works identically with or without manifests
- [ ] All existing plugin functionality (discovery, component loading, tool settings) is unaffected
- [ ] BDD and E2E tests updated to reflect the new default behavior
- [ ] Dry-run reports updated to reflect whether manifests will be generated

### 3.2 Non-Goals (Out of Scope)

- [ ] Workspace root `aipm.toml` (`--workspace` flag) is NOT affected — it remains generated when requested
- [ ] `aipm-pack init` is NOT affected — the author tool always generates a manifest (it's the whole point)
- [ ] No `aipm manifest generate` command in this spec — that will come with the dependency management spec
- [ ] No changes to the manifest module (`manifest/types.rs`, `manifest/validate.rs`) — schema stays the same
- [ ] No environment variable toggle — the `--manifest` flag is the sole control

## 4. Proposed Solution (High-Level Design)

### 4.1 Behavior Change Summary

```
BEFORE (current):
  aipm init                     → .ai/starter-aipm-plugin/aipm.toml ✅ generated
  aipm init --marketplace       → .ai/starter-aipm-plugin/aipm.toml ✅ generated
  aipm init --no-starter        → no starter plugin at all
  aipm migrate                  → .ai/{name}/aipm.toml ✅ generated per plugin
  aipm migrate --dry-run        → no files (report only)

AFTER (proposed):
  aipm init                     → .ai/starter-aipm-plugin/aipm.toml ❌ NOT generated
  aipm init --manifest          → .ai/starter-aipm-plugin/aipm.toml ✅ generated
  aipm init --no-starter        → no starter plugin at all (unchanged)
  aipm migrate                  → .ai/{name}/aipm.toml ❌ NOT generated
  aipm migrate --manifest       → .ai/{name}/aipm.toml ✅ generated per plugin
  aipm migrate --dry-run        → no files (report only, unchanged)
```

Workspace root manifests are unaffected:

```
  aipm init --workspace         → {dir}/aipm.toml ✅ generated (unchanged)
  aipm init --workspace --manifest → {dir}/aipm.toml ✅ + starter aipm.toml ✅
```

### 4.2 Flag Design

**Flag name**: `--manifest`

**Semantics**: When present, generate `aipm.toml` plugin manifests alongside the plugin directory structure. When absent (the default), skip manifest generation.

**Rationale for opt-in polarity**: The dependency management system does not exist yet. The manifests serve no runtime purpose. Generating them by default creates confusion. Once the dependency system ships, the default can be flipped back (or the flag removed entirely).

### 4.3 Key Components

| Component | Change | File(s) |
|-----------|--------|---------|
| CLI argument parsing | Add `--manifest` bool flag to `Init` and `Migrate` variants | `crates/aipm/src/main.rs` |
| Wizard resolution | Thread `manifest` flag through wizard defaults | `crates/aipm/src/wizard.rs` |
| Workspace init Options | Add `manifest: bool` field | `crates/libaipm/src/workspace_init/mod.rs` |
| Migrate Options | Add `manifest: bool` field | `crates/libaipm/src/migrate/mod.rs` |
| `scaffold_marketplace()` | Conditionally skip `generate_starter_manifest()` and its write | `crates/libaipm/src/workspace_init/mod.rs` |
| `emit_plugin()` | Conditionally skip `generate_plugin_manifest()` and its write | `crates/libaipm/src/migrate/emitter.rs` |
| `emit_plugin_with_name()` | Conditionally skip `generate_plugin_manifest()` and its write | `crates/libaipm/src/migrate/emitter.rs` |
| `emit_package_plugin()` | Conditionally skip `generate_package_manifest()` and its write | `crates/libaipm/src/migrate/emitter.rs` |
| Dry-run report | Indicate whether `--manifest` would generate manifests | `crates/libaipm/src/migrate/dry_run.rs` |
| BDD features | Update scenarios asserting `aipm.toml` existence/non-existence | `tests/features/manifest/` |
| E2E tests | Update tests checking for `aipm.toml` files | `crates/aipm/tests/` |

## 5. Detailed Design

### 5.1 CLI Changes (`crates/aipm/src/main.rs`)

Add `manifest` field to both `Commands::Init` and `Commands::Migrate`:

```rust
// Inside Commands::Init (after line 38, the no_starter field)
/// Generate aipm.toml plugin manifests (opt-in; dependency management not yet available).
#[arg(long)]
manifest: bool,

// Inside Commands::Migrate (after line 59, the max_depth field)
/// Generate aipm.toml plugin manifests (opt-in; dependency management not yet available).
#[arg(long)]
manifest: bool,
```

**Help text rationale**: The parenthetical explains *why* this is opt-in, reducing confusion for users who read `--help`.

Thread the flag to the Options struct in each match arm:

```rust
// Init match arm (around line 81-86):
let opts = libaipm::workspace_init::Options {
    dir: &dir,
    workspace: do_workspace,
    marketplace: do_marketplace,
    no_starter: do_no_starter,
    manifest,           // NEW
};

// Migrate match arm (around line 114-118):
let opts = libaipm::migrate::Options {
    dir: &dir,
    source: source.as_deref(),
    dry_run,
    max_depth,
    manifest,           // NEW
};
```

### 5.2 Wizard Changes (`crates/aipm/src/wizard.rs`)

The `manifest` flag is a CLI-only concern — it does not participate in the interactive wizard. The wizard resolves `(workspace, marketplace, no_starter)` but `manifest` is always passed through directly from the CLI flag. No wizard prompt for "generate manifests?" is needed.

The `resolve_defaults()` function (line 150) and `resolve_workspace_answers()` function (line 100) do not need modification. The `manifest` value flows from `main.rs` directly into `Options.manifest`, bypassing the wizard entirely.

### 5.3 Library Options Changes

#### `workspace_init::Options` (`crates/libaipm/src/workspace_init/mod.rs`)

Add field at line ~41:

```rust
pub struct Options<'a> {
    pub dir: &'a Path,
    pub workspace: bool,
    pub marketplace: bool,
    pub no_starter: bool,
    pub manifest: bool,         // NEW
}
```

#### `migrate::Options` (`crates/libaipm/src/migrate/mod.rs`)

Add field at line ~76:

```rust
pub struct Options<'a> {
    pub dir: &'a Path,
    pub source: Option<&'a str>,
    pub dry_run: bool,
    pub max_depth: Option<usize>,
    pub manifest: bool,         // NEW
}
```

### 5.4 Starter Plugin Manifest Suppression (`crates/libaipm/src/workspace_init/mod.rs`)

In `scaffold_marketplace()` (line 171), the `no_starter` flag currently gates the *entire* starter plugin. The `manifest` flag is different — it gates only the `aipm.toml` file within the starter plugin, while all other files (SKILL.md, hooks.json, scripts, agents, `.claude-plugin/plugin.json`) are still created.

The `manifest` flag must be threaded from `init()` into `scaffold_marketplace()`. Change the function signature:

```rust
fn scaffold_marketplace(dir: &Path, no_starter: bool, manifest: bool, fs: &dyn Fs) -> Result<(), Error>
```

Inside `scaffold_marketplace()`, after writing component files (around line 221) and before the manifest write (line 224-225), add a conditional:

```rust
// Generate and write aipm.toml only if --manifest was requested
if manifest {
    let starter_manifest = generate_starter_manifest();
    fs.write_file(&starter.join("aipm.toml"), starter_manifest.as_bytes())?;

    let parsed = crate::manifest::parse_and_validate(&starter_manifest, Some(&starter))
        .map_err(|e| /* existing error conversion */)?;
}
```

When `manifest` is `false`, the starter plugin directory is created with all components and `plugin.json`, but no `aipm.toml`. The `generate_starter_manifest()` function is not called. The round-trip validation is also skipped (nothing to validate).

Update the call site in `init()` at line 111:

```rust
scaffold_marketplace(opts.dir, opts.no_starter, opts.manifest, fs)?;
```

### 5.5 Migrate Emitter Changes (`crates/libaipm/src/migrate/emitter.rs`)

All three emit functions need a `manifest: bool` parameter.

#### `emit_plugin()` (line 28)

Change signature:

```rust
pub fn emit_plugin<S: BuildHasher>(
    artifact: &Artifact,
    ai_dir: &Path,
    existing_names: &HashSet<String, S>,
    rename_counter: &mut u32,
    manifest: bool,             // NEW
    fs: &dyn Fs,
) -> Result<(String, Vec<Action>), Error>
```

Guard the manifest generation block (lines 97-99):

```rust
// Generate aipm.toml only if --manifest was requested
if manifest {
    let toml = generate_plugin_manifest(artifact, &plugin_name);
    write_file(&plugin_dir.join("aipm.toml"), &toml, fs)?;
}
```

The `plugin.json` generation (lines 102-103) is NOT guarded — it is required for Claude Code plugin discovery.

#### `emit_plugin_with_name()` (line 229)

Change signature:

```rust
pub fn emit_plugin_with_name(
    artifact: &Artifact,
    plugin_name: &str,
    ai_dir: &Path,
    manifest: bool,             // NEW
    fs: &dyn Fs,
) -> Result<Vec<Action>, Error>
```

Guard the manifest write at line 298 with `if manifest { ... }`.

#### `emit_package_plugin()` (line 317)

Change signature:

```rust
pub fn emit_package_plugin(
    plugin_name: &str,
    artifacts: &[Artifact],
    ai_dir: &Path,
    manifest: bool,             // NEW
    fs: &dyn Fs,
) -> Result<Vec<Action>, Error>
```

Guard the manifest generation block at lines 416-424 with `if manifest { ... }`.

### 5.6 Migrate Orchestrator Changes (`crates/libaipm/src/migrate/mod.rs`)

Thread `manifest` from `Options` to emit function calls.

#### `migrate_single_source()` (line 196)

Change signature to accept `manifest: bool`. At the `emit_plugin()` call site (line 236), pass it through:

```rust
let (name, mut plugin_actions) = emitter::emit_plugin(
    artifact,
    &ai_dir,
    &known_names,
    &mut rename_counter,
    manifest,           // NEW
    fs,
)?;
```

#### `migrate_recursive()` (line 250)

Change signature to accept `manifest: bool`. At the emission parallel block (lines 336-354), pass it to both emit functions:

```rust
// Package-scoped path (line 343):
emitter::emit_package_plugin(&plan.name, &plan.artifacts, &ai_dir, manifest, fs)

// Single-artifact path (line 348):
emitter::emit_plugin_with_name(first, &plan.name, &ai_dir, manifest, fs)
```

#### `migrate()` (line 181)

Pass `opts.manifest` to the internal functions:

```rust
opts.source.map_or_else(
    || migrate_recursive(opts.dir, opts.max_depth, opts.dry_run, opts.manifest, &ai_dir, fs),
    |source| migrate_single_source(opts.dir, source, opts.dry_run, opts.manifest, &ai_dir, fs),
)
```

### 5.7 Dry-Run Report Changes (`crates/libaipm/src/migrate/dry_run.rs`)

The dry-run report currently includes a line per artifact showing `New aipm.toml with type = "..."` (line 213). This line should be conditionally included based on the `manifest` flag.

Thread `manifest: bool` into the report generation function. When `manifest` is `false`, either:
- Omit the `aipm.toml` line entirely, or
- Replace it with: `  - No aipm.toml (pass --manifest to generate)`

The latter is preferred as it educates the user about the flag.

### 5.8 Registration Behavior (No Changes)

The registrar (`registrar.rs:10-47`) appends entries to `marketplace.json` based on the plugin directory existing. It does NOT read `aipm.toml`. Therefore, plugin registration works identically with or without manifests. **No changes needed in `registrar.rs`.**

### 5.9 Action Enum (No Changes)

The `Action::PluginCreated` variant (mod.rs:83-90) reports `plugin_type: String`. This type is derived from the artifact kind, not from the manifest. It works the same with or without manifest generation. **No changes needed in the Action enum.**

## 6. Alternatives Considered

| Option | Pros | Cons | Reason for Rejection |
|--------|------|------|---------------------|
| **`--no-manifest` (opt-out)** | Backward compatible; no test changes for default path | Confusing default when dependency management doesn't exist; double-negative flag name | Users shouldn't need to know about a system that doesn't work yet |
| **Environment variable toggle** | CI-friendly; global override | Hidden behavior; harder to discover; another config surface | A CLI flag is discoverable via `--help` and sufficient |
| **Remove manifest generation entirely** | Simplest change | Loses the ability to generate manifests for users who want them (e.g., testing, early adoption) | Too aggressive; some users may want manifests for experimentation |
| **Suppress only during migrate** | Smaller change surface | `aipm init` starter plugin has the same confusion problem | Inconsistent — same confusing manifest appears in both commands |

## 7. Cross-Cutting Concerns

### 7.1 Backward Compatibility

This is a **behavioral breaking change** for the default path. Users who previously relied on `aipm migrate` or `aipm init` generating `aipm.toml` files will no longer get them unless they pass `--manifest`.

**Mitigation**: Since `aipm.toml` serves no runtime purpose today (Claude Code ignores it), no existing workflows should break. Users who have already generated manifests keep them — this change only affects *future* invocations.

### 7.2 Documentation

The `--help` text for `--manifest` should explain why it's opt-in: `"Generate aipm.toml plugin manifests (opt-in; dependency management not yet available)."` When the dependency system ships, this help text should be updated or the flag removed.

### 7.3 Future Reconciliation

When marketplace linking/dependency management is implemented, a separate `aipm manifest generate` command will scan `.ai/` for plugin directories without `aipm.toml` and generate manifests from their component files. This is explicitly out of scope for this spec but noted here for design continuity.

At that point, the `--manifest` flag default may also flip back to opt-out (`--no-manifest`) or the flag may be removed entirely.

## 8. Test Plan

### 8.1 BDD Feature Changes (`tests/features/manifest/`)

#### `migrate.feature`

**Scenarios asserting `aipm.toml` existence** (lines 6, 64) — these must be updated:

- **Scenario "Migrate a single skill"** (line 6): Currently asserts `.ai/deploy/aipm.toml` contains `name = "deploy"`. Change to:
  1. Assert `.ai/deploy/aipm.toml` does **NOT** exist (default behavior)
  2. Add a new scenario: "Migrate a single skill with --manifest" that runs `aipm migrate --manifest` and asserts `.ai/deploy/aipm.toml` exists with correct content

- **Scenario "Recursive discovery finds skill in sub-package"** (line 64): Currently asserts `.ai/auth/aipm.toml` contains `name = "auth"`. Change to:
  1. Assert `.ai/auth/aipm.toml` does **NOT** exist (default behavior)
  2. Add a new scenario: "Recursive discovery with --manifest" that asserts the manifest is generated when the flag is passed

**New BDD scenarios to add:**

```gherkin
Scenario: Default migrate does not generate aipm.toml
  Given an empty directory "my-project"
  And a workspace initialized in "my-project"
  And a skill "deploy" exists in "my-project"
  When the user runs "aipm migrate" in "my-project"
  Then the command succeeds
  And a plugin directory exists at ".ai/deploy/" in "my-project"
  And there is no file ".ai/deploy/aipm.toml" in "my-project"
  And a file ".ai/deploy/skills/deploy/SKILL.md" exists in "my-project"
  And the marketplace.json in "my-project" contains plugin "deploy"

Scenario: Migrate with --manifest generates aipm.toml
  Given an empty directory "my-project"
  And a workspace initialized in "my-project"
  And a skill "deploy" exists in "my-project"
  When the user runs "aipm migrate --manifest" in "my-project"
  Then the command succeeds
  And the file ".ai/deploy/aipm.toml" in "my-project" contains 'name = "deploy"'
  And the file ".ai/deploy/aipm.toml" in "my-project" contains 'type = "skill"'

Scenario: Recursive migrate without --manifest skips aipm.toml
  Given an empty directory "my-project"
  And a workspace initialized in "my-project"
  And a skill "deploy" exists in sub-package "auth" of "my-project"
  When the user runs "aipm migrate" in "my-project"
  Then the command succeeds
  And a plugin directory exists at ".ai/auth/" in "my-project"
  And there is no file ".ai/auth/aipm.toml" in "my-project"
  And the marketplace.json in "my-project" contains plugin "auth"

Scenario: Recursive migrate with --manifest generates aipm.toml
  Given an empty directory "my-project"
  And a workspace initialized in "my-project"
  And a skill "deploy" exists in sub-package "auth" of "my-project"
  When the user runs "aipm migrate --manifest" in "my-project"
  Then the command succeeds
  And the file ".ai/auth/aipm.toml" in "my-project" contains 'name = "auth"'
```

#### `workspace-init.feature`

**Scenarios asserting starter `aipm.toml` existence** (lines 30, 131) — update:

- **Scenario "Marketplace generates a valid starter plugin manifest"** (line 30): Change to assert `.ai/starter-aipm-plugin/aipm.toml` does NOT exist by default. Add a new scenario with `--manifest` flag that asserts it does.

- **Scenario "Starter plugin manifest is valid TOML"** (line 131): Move under `--manifest` flag variant.

**Scenarios asserting root `aipm.toml`** (lines 8, 15, 81, 116) — **NO CHANGES**. These test `--workspace` which is unaffected.

**New BDD scenarios to add:**

```gherkin
Scenario: Default init does not generate starter aipm.toml
  Given an empty directory "my-project"
  When the user runs "aipm init --marketplace" in "my-project"
  Then a file ".ai/starter-aipm-plugin/skills/scaffold-plugin/SKILL.md" exists in "my-project"
  And there is no file ".ai/starter-aipm-plugin/aipm.toml" in "my-project"

Scenario: Init with --manifest generates starter aipm.toml
  Given an empty directory "my-project"
  When the user runs "aipm init --marketplace --manifest" in "my-project"
  Then a file ".ai/starter-aipm-plugin/aipm.toml" exists in "my-project"
  And the starter plugin manifest contains the package name "starter-aipm-plugin"
  And the starter plugin manifest is valid according to aipm schema

Scenario: Workspace flag is independent of --manifest
  Given an empty directory "my-project"
  When the user runs "aipm init --workspace" in "my-project"
  Then a file "aipm.toml" is created in "my-project"
  And there is no directory ".ai/starter-aipm-plugin" in "my-project"
```

#### `init.feature` — **NO CHANGES**

The `aipm-pack init` command is unaffected. All existing scenarios remain as-is.

### 8.2 E2E Test Changes (`crates/aipm/tests/`)

#### `migrate_e2e.rs`

| Test Function | Change |
|--------------|--------|
| `migrate_skill_creates_plugin` (line 38) | Assert `aipm.toml` does NOT exist; add separate `_with_manifest` variant |
| `migrate_command_creates_plugin` (line 60) | Assert `aipm.toml` does NOT exist |
| `migrate_multiple_skills` (line 253) | Assert `aipm.toml` does NOT exist for either plugin |
| `migrate_skill_with_scripts` (line 274) | Assert `aipm.toml` does NOT exist |
| `migrate_dry_run_no_side_effects` (line 177) | No change (already asserts no files) |

**New E2E tests:**

```rust
#[test]
fn migrate_with_manifest_flag_generates_toml() { ... }

#[test]
fn migrate_without_manifest_flag_skips_toml() { ... }

#[test]
fn migrate_manifest_flag_with_recursive_discovery() { ... }
```

#### `init_e2e.rs`

| Test Function | Change |
|--------------|--------|
| `init_default_creates_marketplace_only` (line 22) | Assert starter `aipm.toml` does NOT exist |
| `init_marketplace_only` (line 54) | Assert starter `aipm.toml` does NOT exist |
| `init_starter_manifest_valid_toml` (line 140) | Change to use `--manifest` flag |
| `yes_flag_creates_default_marketplace` (line 372) | Assert starter `aipm.toml` does NOT exist |

**New E2E tests:**

```rust
#[test]
fn init_manifest_flag_generates_starter_toml() { ... }

#[test]
fn init_without_manifest_flag_skips_starter_toml() { ... }
```

### 8.3 Unit Test Changes (`crates/libaipm/src/`)

#### `workspace_init/mod.rs` (unit tests section)

Tests that assert `aipm.toml` existence for the starter plugin must be updated to pass `manifest: true` in the `Options` struct, or split into two variants (one with, one without).

Key tests to update:
- Test asserting `.ai/starter-aipm-plugin/aipm.toml` exists (line ~569)
- Test asserting both root and starter `aipm.toml` exist (line ~621)

#### `migrate/emitter.rs` (unit tests section)

All emitter unit tests use `MockFs`. Tests that assert `aipm.toml` was written must be updated:
- Tests checking `fs.get_written(Path::new("/ai/deploy/aipm.toml")).is_some()` should be split: one variant with `manifest: true` (asserts written), one with `manifest: false` (asserts NOT written).

Key tests to update (by line):
- Line ~734: `emit_plugin` basic test
- Line ~750: `emit_plugin` content assertion
- Line ~1276: `emit_plugin_with_name` test
- Line ~1333: `emit_package_plugin` test
- Line ~1359: composite type assertion
- Line ~1378: hooks merge test
- Line ~1397: scripts test
- Line ~1424: command-to-skill conversion

### 8.4 Snapshot Tests

The scaffold script snapshot (`workspace_init/snapshots/libaipm__workspace_init__tests__scaffold_script_snapshot.snap`) contains the generated `scaffold-plugin.ts` which writes `aipm.toml` when creating new plugins via the TypeScript scaffold script. This snapshot is **not affected** — the scaffold script is a *user-facing tool* that creates new plugins at runtime, and its behavior is independent of the `--manifest` CLI flag.

### 8.5 BDD Step Definitions (`crates/libaipm/tests/bdd.rs`)

A new step definition is needed:

```gherkin
And there is no file "{filename}" in "{dir}"
```

Check if this step already exists (it is used in `workspace-init.feature` line 108). If so, reuse it. If not, add it as a `Then` step that asserts `!path.join(filename).exists()`.

## 9. Open Questions / Unresolved Issues

- [ ] **Dry-run wording**: Exact text for the dry-run report line when `--manifest` is not set. Proposed: `"  - No aipm.toml (pass --manifest to generate)"`. Review during implementation.
- [ ] **Future default flip**: When the dependency system ships, should `--manifest` become the default and `--no-manifest` be added? Or should the flag be removed entirely? Deferred to the dependency management spec.
- [ ] **`aipm manifest generate` command**: The reconciliation path for plugins migrated without manifests. Deferred to a separate spec when dependency management is implemented.
