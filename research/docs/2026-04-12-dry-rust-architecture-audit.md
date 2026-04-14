---
date: 2026-04-12 00:00:00 UTC
researcher: Claude (Opus 4.6)
git_commit: 2f7e055ad69f77b7df6a19fd5d582a34af79dd44
branch: main
repository: aipm
topic: "DRY and Rust best-practices audit before adding aipm make (#363), CLI scaffolding (#361), init/lint consistency (#356)"
tags: [research, architecture, dry, rust-idioms, scaffolding, lint, manifest, deduplication, pre-complexity]
status: complete
last_updated: 2026-04-12
last_updated_by: Claude (Opus 4.6)
---

# Architecture Audit: DRY Analysis and Rust Best Practices

## Research Question

Conduct a comprehensive DRY and Rust best-practices audit of the aipm codebase, documenting: (1) duplicated logic patterns across crates — especially in scaffolding, file I/O, manifest manipulation, and engine adapter code; (2) Rust idiom adherence (error handling, trait design, module organization, type-state patterns); (3) opportunities where shared abstractions could reduce surface area before adding the `aipm make` foundational API (#363), the CLI-driven plugin scaffolding (#361), and the init/lint consistency fix (#356).

**GitHub Issues covered:**
- [TheLarkInn/aipm#363](https://github.com/TheLarkInn/aipm/issues/363) — `aipm make` foundational scaffolding API
- [TheLarkInn/aipm#361](https://github.com/TheLarkInn/aipm/issues/361) — Starter plugin using `aipm` CLI instead of TypeScript
- [TheLarkInn/aipm#356](https://github.com/TheLarkInn/aipm/issues/356) — Starter plugin fails default `aipm lint` checks

---

## Summary

The codebase is **not DRY** in several important areas, particularly around manifest generation, file write patterns, and the lint rule layer. The Rust idiom adherence is **mostly good** at the trait and type level but is **inconsistent** in error handling and filesystem abstraction. Before adding the complexity of `aipm make`, there are 10 distinct deduplication targets that would reduce the surface area of the new API and prevent the same duplication from being baked into the next layer.

The most consequential duplications relative to the three issues are:
- Four independent `aipm.toml` generation paths (critical for #363)
- Two independent `plugin.json` generation paths (critical for #361, #356)
- Four independent package/name validators (structural risk for make actions)
- Lint rule `check()`/`check_file()` boilerplate (blocks clean extensibility for #356 fix)
- Scattered `marketplace.json` read-modify-write logic (the backbone of #363's make actions)

---

## Detailed Findings

### A. Crate Structure Overview

Three crates, 95 source files in `libaipm`, 5 in `aipm` CLI, 3 in `aipm-pack`:

```
crates/
  aipm/          — consumer CLI binary (init, install, update, link, uninstall, unlink, list, lint, migrate, lsp)
  aipm-pack/     — author CLI binary (init only)
  libaipm/       — core library (27 public modules)
```

**Cross-crate API surface** (`aipm` depends on): `workspace_init`, `installer::pipeline`, `linker::*`, `lockfile`, `lint`, `migrate`, `logging`, `fs::Real`, `manifest::load`, `workspace::find_workspace_root`, `cache::Policy`, `locked_file::LockedFile`, `installed::Registry`.

**Cross-crate API surface** (`aipm-pack` depends on): only `init::{Options, init}`, `manifest::types::PluginType`, `fs::Real`, and `version()`.

### B. DRY Violations — Inventory

#### B1. aipm.toml Manifest Generation — 4 Independent Approaches

The `Manifest` struct in `manifest/types.rs` derives `Deserialize` but **not** `Serialize`. This forces every TOML generation path to invent its own approach.

| Location | Approach | What it generates |
|---|---|---|
| [`init.rs:187`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/init.rs#L187) | `format!()` string | `[package]` with name, version, type |
| [`workspace_init/mod.rs:275`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/workspace_init/mod.rs#L275) | string literal | `[package]` + `[components]` for starter plugin |
| [`workspace_init/mod.rs:166`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/workspace_init/mod.rs#L166) | string literal | `[workspace]` with members, plugins_dir, comment blocks |
| [`migrate/emitter.rs:1113`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/migrate/emitter.rs#L1113) | private `PluginToml` + `toml::to_string_pretty` | `[package]` + `[components]` with populate fields |
| `workspace_init/mod.rs:365` (embedded JS) | JavaScript template literal | `[package]` + `[components]` at runtime |

Root cause: The emitter defines its own `PluginToml`, `PluginPackage`, `PluginComponents` structs (lines 1080–1110) because `manifest::types::Manifest` cannot be serialized. These private structs duplicate the concept of `Package`/`Components` from `manifest/types.rs`.

#### B2. plugin.json Generation — 2 Independent Implementations

| Location | Context | Fields |
|---|---|---|
| [`workspace_init/mod.rs:294`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/workspace_init/mod.rs#L294) | init — starter plugin | name, version, description, author |
| [`migrate/emitter.rs:1183`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/migrate/emitter.rs#L1183) | migrate — all plugin types | Same base + skills, agents, mcpServers, hooks, outputStyles, lspServers, extensions |

Both hardcode `version: "0.1.0"` and `author: {name: "TODO", email: "TODO"}`. Both use `serde_json::Map::new()` + `to_string_pretty` + trailing newline.

This is directly implicated in **#356**: `generate_plugin_json()` (workspace_init path) generates a plugin.json without the component fields that `plugin/required-fields` lint rule expects.

#### B3. Package/Name Validation — 4 Independent Implementations

| Location | Type | Rules enforced |
|---|---|---|
| [`manifest/validate.rs:14`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/manifest/validate.rs#L14) | `is_valid_name` + `is_valid_segment` | Structural: scoped `@org/name`, first-char must be `[a-z0-9]`, only hyphens |
| [`init.rs:99`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/init.rs#L99) | `is_valid_package_name` + `is_valid_segment` | Identical rules to #1; independent copy |
| [`aipm-pack/wizard.rs:168`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/aipm-pack/src/wizard.rs#L168) | `validate_package_name` | Character-set only; allows empty (uses default); no first-char rule |
| [`aipm/wizard.rs:211`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/aipm/src/wizard.rs#L211) | `validate_marketplace_name` | Identical to #3; different name |

The wizard validators (#3, #4) and the library validators (#1, #2) have diverged rules. `init.rs` has its own copy of the `manifest/validate.rs` logic that is not called from there — they exist independently.

#### B4. marketplace.json Read-Modify-Write — 2 Independent Implementations

| Location | Language | Checks for duplicates? |
|---|---|---|
| [`migrate/registrar.rs:10`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/migrate/registrar.rs#L10) | Rust | Yes (name match at line 31) |
| `workspace_init/mod.rs:380` (generated TypeScript) | TypeScript | Yes |

Both follow: read → parse → find plugins array → check for existing entry → push → stringify → write. The Rust version is the canonical implementation; the TypeScript version was generated to run at install-time in the starter plugin scaffold script.

The `aipm make` feature (#363) will need a third (canonical Rust) implementation that can be called from both contexts.

#### B5. .claude/settings.json Read-Modify-Write — 2 Independent Implementations

| Location | Language | What it writes |
|---|---|---|
| [`workspace_init/adaptors/claude.rs:61`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/workspace_init/adaptors/claude.rs#L61) | Rust | `extraKnownMarketplaces` + `enabledPlugins` |
| `workspace_init/mod.rs:407` (generated TypeScript) | TypeScript | `enabledPlugins` only |

These handle overlapping but different fields. The Rust adaptor handles both; the TypeScript scaffold script only adds an `enabledPlugins` entry for the newly scaffolded plugin.

#### B6. YAML Frontmatter Parsing — 2 Independent Implementations

| Location | Returns | Fields extracted |
|---|---|---|
| [`frontmatter.rs:39`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/frontmatter.rs#L39) | `Frontmatter` (all fields, line numbers, body) | Everything |
| [`migrate/skill_common.rs:13`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/migrate/skill_common.rs#L13) | `ArtifactMetadata` (name, description, hooks, disable-model-invocation) | 4 specific fields |

Both use the same `---` delimiter detection and line-scan approach. The migrate version predates or was written independently of `frontmatter.rs`.

#### B7. wizard.rs / wizard_tty.rs Pattern Duplicated Across Binaries

Both `crates/aipm/` and `crates/aipm-pack/` contain structurally parallel modules:

```
crates/aipm/src/wizard.rs         — PromptStep, PromptKind, PromptAnswer, styled_render_config, validate_marketplace_name
crates/aipm-pack/src/wizard.rs    — PromptStep, PromptKind, PromptAnswer, styled_render_config, validate_package_name
```

These types and the `styled_render_config` function are independent copies, not shared from `libaipm`. As `aipm make` adds more interactive scaffold paths, the wizard infrastructure will grow and diverge further.

#### B8. "Create Parent Dir Then Write" Pattern — 6+ Independent Implementations

The idiom `if let Some(parent) = path.parent() { create_dir_all(parent)?; } write(path, content)?;` appears independently in:

- [`lockfile/mod.rs:59`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/lockfile/mod.rs#L59) — direct `std::fs`
- [`linker/link_state.rs:55`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/linker/link_state.rs#L55) — direct `std::fs`
- [`linker/gitignore.rs:173`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/linker/gitignore.rs#L173) — direct `std::fs`
- [`locked_file.rs:27`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/locked_file.rs#L27) — direct `std::fs`
- [`fs.rs:176` (`atomic_write`)](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/fs.rs#L176) — direct `std::fs`
- [`cache.rs:214`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/cache.rs#L214) — direct `std::fs`

Note that `Fs::create_dir_all` + `Fs::write_file` together do cover this in scaffolding code that uses the trait, but these 6+ locations bypass the `Fs` trait and use `std::fs` directly.

#### B9. "Read JSON/TOML, Default on Missing" Pattern — 4 Locations

The idiom of checking for file existence (or catching `NotFound`) and returning a default value appears in:

- [`main.rs:656`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/aipm/src/main.rs#L656) — `load_installed_registry` JSON
- [`cache.rs:376`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/cache.rs#L376) — `read_index` JSON
- [`linker/link_state.rs:37`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/linker/link_state.rs#L37) — TOML (catches `NotFound`)
- [`linker/gitignore.rs:90`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/linker/gitignore.rs#L90) — plain text (catches `NotFound`)

#### B10. Lint Rule Boilerplate — 8 Patterns of Internal Duplication

Within the lint module itself, several boilerplate blocks are repeated:

| Boilerplate | Where | Count |
|---|---|---|
| `check()` + `check_file()` duplicate validation logic | All 7 skill rules + `agent_missing_tools` | 8 rules |
| `locate_json_key()` helper function | `hook_unknown_event.rs:21`, `hook_legacy_event.rs:15` | 2 (identical) |
| Private `diag()` helper (hardcoded rule_id) | 4 marketplace/plugin rules | 4 |
| Field-value column range computation | 4 skill rules × 2 methods | 8 occurrences |
| `check_file()` preamble (source_type + read_skill) | 7 skill rules | 7 |
| JSON parse + hooks extraction in hook rules | 2 rules × 2 methods | 4 |
| Marketplace `check()`/`check_file()` wrapper | 3 marketplace/plugin rules | 3 |
| Legacy inline `MockFs` | `skill_missing_name.rs:125` | 1 (vs shared in test_helpers) |

The two-tier discovery architecture (B10a) also creates implicit duplication: `discovery.rs` does a gitignore-aware full walk that classifies all files, but lint rules that use `check()` (the legacy path) then walk `.ai/` again via `scan.rs`. The `check_file()` pipeline avoids the second walk — but migration to the unified pipeline is incomplete (all 18+ rules still implement both methods).

#### B11. Direct std::fs vs Fs Trait — Inconsistent Filesystem Abstraction

The `Fs` trait was introduced to enable mock-based testing in lint rules and scaffolding. However, these modules bypass it and call `std::fs` directly:

- `lockfile/mod.rs` (read + write)
- `linker/link_state.rs` (read + write)
- `linker/gitignore.rs` (read + write)
- `installer/manifest_editor.rs` (read + write)
- `installer/pipeline.rs` (read)
- `workspace/mod.rs` (read)
- `manifest/mod.rs` (read)
- `locked_file.rs` (open)
- `cache.rs` (read)
- `aipm/src/main.rs` (read for installed registry)

These modules are not currently tested with mock file systems. Adding `aipm make` actions over these modules will continue the pattern unless the abstraction is extended.

---

### C. Rust Idiom Adherence

#### C1. Error Handling — Mostly Good, Inconsistent Location

**Good**: All errors use `#[derive(thiserror::Error)]`. The `?` operator and `.map_err()` are used throughout; no `unwrap()` or `panic!()` in production code (enforced by lints).

**Inconsistency 1 — Location**: 8 modules use dedicated `error.rs` files; 14+ modules define their error enum inline in `mod.rs` or the owning file. There is no rule for which approach to use.

| Pattern | Modules |
|---|---|
| Dedicated `error.rs` | `workspace`, `resolver`, `store`, `registry`, `lockfile`, `manifest`, `linker`, `installer` |
| Inline enum | `lint/mod.rs`, `migrate/mod.rs`, `workspace_init/mod.rs`, `init.rs`, `spec.rs`, `version.rs`, `engine.rs`, `locked_file.rs`, `acquirer.rs`, `discovery.rs`, `security.rs`, `path_security.rs`, `logging.rs`, `cache.rs`, `marketplace.rs` |

**Inconsistency 2 — Conversion**: Some errors use `#[from]`/`#[error(transparent)]` for automatic conversion (e.g., `init.rs:43`, `spec.rs:36`, `lint/mod.rs:197`). Most use explicit `.map_err(|e| Error::Variant { ... })`. The two approaches are mixed within the same modules.

**Inconsistency 3 — CLI entry point**: Both CLI binaries' `run()` functions use `Box<dyn std::error::Error>` as the return type (e.g., `aipm/src/main.rs:956`), erasing error type information. Library errors are converted via `.to_string()` + `std::io::Error::other(e.to_string())` throughout `main.rs`. This pattern repeats approximately 15 times in `main.rs`.

#### C2. Trait Design — Good, One Gap

All six domain traits (`Fs`, `Rule`, `Detector`, `ToolAdaptor`, `Registry`, `Reporter`) are well-designed:
- Zero-sized or minimally-sized implementors
- `&dyn Trait` objects (no monomorphization cost)
- Factory functions returning `Vec<Box<dyn Trait>>`
- `Send + Sync` on traits that need parallelism

**Gap**: The `ToolAdaptor` trait is only used during `aipm init`. No adaptor exists for engine-specific install, link, or lint behavior. As new engines are added, install/link/lint will need engine-specific paths — there is no abstraction ready for this yet.

**Gap**: `Fs::remove_file`, `Fs::remove_dir_all`, `Fs::hard_link`, `Fs::copy_file`, `Fs::symlink_dir`, etc. exist as "not implemented" default stubs returning `Err(io::Error::other("not implemented"))`. This is a design smell — these should either be in a separate `FsExt` trait or be required methods. Currently, calling them on `Real` works but calling them on `MockFs` silently fails.

#### C3. Module Organization — Good

The `mod.rs`-plus-submodules pattern is used consistently for complex domains. Flat file modules are used for simpler ones. `pub(crate)` is used appropriately for internal helpers (`scan`, `test_helpers`, `quality_rules_for_kind`).

One inconsistency: `lib.rs` exports all modules as `pub mod` without any re-exports of commonly used types. Consumers must always write `libaipm::manifest::types::Manifest` rather than `libaipm::Manifest`. This is intentional (no barrel exports) but worth noting.

#### C4. Type Patterns — Good

- Newtypes for domain constraints: `Version(semver::Version)`, `ValidatedPath(String)` — correct and used
- Enums with data: `Spec`, `DependencySpec`, `RuleOverride`, `ArtifactKind` — idiomatic
- `FromStr`/`Display`/`Serialize`/`Deserialize` via `FromStr`+`Display` round-trip on `Spec`, `Policy` — correct
- Lifetime-parameterized `Options<'a>` structs for zero-copy config passing — idiomatic

**Gap**: No builder pattern exists. `Options` structs use field initialization (`Options { dir, workspace: true, ... }`). This is fine for small structs but may become unwieldy as `aipm make` adds more options.

#### C5. Iterator Patterns — Good

Modern Rust idioms are used throughout: `.is_some_and()`, `.and_then()` chains, `.filter().collect()`, `.sort_by().then_with()`, `.map_or_else()`, `.transpose()`. No hand-rolled loops where iterators would be clearer.

---

### D. Two-Tier Discovery Architecture (Lint-Specific)

The lint pipeline has an architectural redundancy worth documenting before extending it for `aipm make lint --fix`:

**Tier 1** (`discovery.rs:discover_features()`): Called once at line 122 of `lint/mod.rs`. Performs a gitignore-aware recursive walk of the project tree via the `ignore` crate. Classifies every matched file into a `FeatureKind`. This is the **intended unified pipeline** path.

**Tier 2** (`lint/rules/scan.rs`): The `check()` method on each rule does its own **second walk** of `.ai/` using `fs.read_dir()`. This is the **legacy path** that exists because `check()` was the original API before `check_file()` was added.

Rules that implement `check_file()` (the Tier 1 path) avoid the double walk. But all 18 rules still implement *both* methods, creating the boilerplate documented in B10. Completing the migration to Tier 1 only (removing `check()` and its scan.rs dependency) would eliminate the redundant walk and all the `check()`/`check_file()` duplication.

---

### E. Relevance to Issues #363, #361, #356

#### E.1 Issue #363 — aipm make foundational API

The primitive operations `aipm make` needs already exist, scattered:

| Action | Current implementation |
|---|---|
| Create plugin directory tree | `init::create_directory_layout` (`init.rs:131`) |
| Write `aipm.toml` | 4 independent paths (see B1) |
| Write `plugin.json` | 2 independent paths (see B2) |
| Register plugin in `marketplace.json` | `migrate::registrar::register_plugins` (`registrar.rs:10`) |
| Enable plugin in `.claude/settings.json` | `workspace_init::adaptors::claude` (`claude.rs:19`) |
| Write `SKILL.md` / template content | `init::create_skill_template` (`init.rs:173`), `workspace_init::scaffold_marketplace` |
| Validate plugin name | 4 independent validators (see B3) |

**Before implementing #363**: Consolidating B1, B2, B3, B4, and B5 into single functions would give `aipm make` a clean set of atomic primitives to compose, rather than adding a 5th/3rd/5th implementation of each.

#### E.2 Issue #361 — Replace TypeScript scaffold with `aipm` CLI

The TypeScript scaffold script at `workspace_init/mod.rs:342` (`generate_scaffold_script()`) is the "random TypeScript" that #361 wants to replace. It currently:
- Creates plugin directory structure
- Writes `aipm.toml`, `SKILL.md`, `plugin.json` (via JS template literals)
- Reads and updates `marketplace.json`
- Reads and updates `.claude/settings.json`

All of these operations have Rust implementations in `libaipm`. The script exists because at init-time there was no `aipm make` CLI to call instead. Issue #361 is directly dependent on #363 being resolved first.

#### E.3 Issue #356 — Starter plugin fails lint checks

The starter plugin files created by `scaffold_marketplace()` (`workspace_init/mod.rs:191`) include:

| File created | Generator | Known lint risk |
|---|---|---|
| `.ai/.claude-plugin/marketplace.json` | `generate_marketplace_json()` line 477 | `marketplace/source-resolve` checks source paths exist |
| `.ai/starter-aipm-plugin/.claude-plugin/plugin.json` | `generate_plugin_json()` line 294 | `plugin/required-fields` — missing component fields |
| `.ai/starter-aipm-plugin/skills/scaffold-plugin/SKILL.md` | string literals | `skill/missing-name`, `skill/missing-description` — need frontmatter |
| `.ai/starter-aipm-plugin/aipm.toml` (when `manifest=true`) | `generate_starter_manifest()` line 275 | `plugin/missing-manifest` doesn't apply (file exists) |
| `.ai/starter-aipm-plugin/agents/marketplace-scanner.md` | string literals | `agent/missing-tools` — needs `tools` frontmatter |

The root cause is that `generate_plugin_json()` (workspace_init path) only produces `{name, version, description, author}` while `generate_plugin_json_multi()` (emitter path) adds component arrays. The plugin.json generated during `init` therefore fails `plugin/required-fields`.

Fix path: Unify B2 first, then have `generate_plugin_json()` accept an optional components argument.

---

## Code References

### DRY Violations

- [`crates/libaipm/src/init.rs:99-128`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/init.rs#L99) — `is_valid_package_name` (duplicate of manifest/validate.rs)
- [`crates/libaipm/src/init.rs:187-203`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/init.rs#L187) — `generate_manifest()` (format! approach)
- [`crates/libaipm/src/manifest/validate.rs:14-52`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/manifest/validate.rs#L14) — canonical name validator
- [`crates/libaipm/src/manifest/types.rs:10`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/manifest/types.rs#L10) — `Manifest` struct (Deserialize only, no Serialize)
- [`crates/libaipm/src/workspace_init/mod.rs:166-292`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/workspace_init/mod.rs#L166) — workspace manifest + starter plugin generators
- [`crates/libaipm/src/workspace_init/mod.rs:294`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/workspace_init/mod.rs#L294) — `generate_plugin_json()` (minimal, no components)
- [`crates/libaipm/src/workspace_init/mod.rs:342`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/workspace_init/mod.rs#L342) — `generate_scaffold_script()` (TypeScript to be replaced by #361)
- [`crates/libaipm/src/workspace_init/mod.rs:477`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/workspace_init/mod.rs#L477) — `generate_marketplace_json()`
- [`crates/libaipm/src/workspace_init/adaptors/claude.rs:19`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/workspace_init/adaptors/claude.rs#L19) — Claude settings.json adaptor
- [`crates/libaipm/src/migrate/emitter.rs:1080-1172`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/migrate/emitter.rs#L1080) — `PluginToml` structs + `generate_plugin_manifest()`
- [`crates/libaipm/src/migrate/emitter.rs:1183`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/migrate/emitter.rs#L1183) — `generate_plugin_json_multi()` (full components)
- [`crates/libaipm/src/migrate/registrar.rs:10`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/migrate/registrar.rs#L10) — `register_plugins()` (Rust marketplace.json update)
- [`crates/libaipm/src/migrate/skill_common.rs:13`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/migrate/skill_common.rs#L13) — independent frontmatter parser
- [`crates/libaipm/src/frontmatter.rs:39`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/frontmatter.rs#L39) — canonical frontmatter parser
- [`crates/libaipm/src/lint/rules/hook_unknown_event.rs:21`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/lint/rules/hook_unknown_event.rs#L21) — `locate_json_key()` (duplicated in hook_legacy_event.rs)
- [`crates/libaipm/src/lint/rules/marketplace_source_resolve.rs:59`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/lint/rules/marketplace_source_resolve.rs#L59) — `diag()` helper (4 private copies)
- [`crates/aipm/src/wizard.rs:211`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/aipm/src/wizard.rs#L211) — `validate_marketplace_name` (char-set only)
- [`crates/aipm-pack/src/wizard.rs:168`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/aipm-pack/src/wizard.rs#L168) — `validate_package_name` (char-set only)

### Trait Definitions

- [`crates/libaipm/src/fs.rs:25`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/fs.rs#L25) — `Fs` trait
- [`crates/libaipm/src/lint/rule.rs:16`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/lint/rule.rs#L16) — `Rule` trait
- [`crates/libaipm/src/migrate/detector.rs:13`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/migrate/detector.rs#L13) — `Detector` trait
- [`crates/libaipm/src/workspace_init/mod.rs:17`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/workspace_init/mod.rs#L17) — `ToolAdaptor` trait
- [`crates/libaipm/src/registry/mod.rs:86`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/registry/mod.rs#L86) — `Registry` trait
- [`crates/libaipm/src/lint/reporter.rs:13`](https://github.com/TheLarkInn/aipm/blob/2f7e055ad69f77b7df6a19fd5d582a34af79dd44/crates/libaipm/src/lint/reporter.rs#L13) — `Reporter` trait

---

## Architecture Documentation

### Current Scaffolding Data Flow

```
aipm init
  └─→ wizard_tty::resolve() [aipm/wizard_tty.rs]
        └─→ libaipm::workspace_init::init() [workspace_init/mod.rs:118]
              ├─→ init_workspace() → generate_workspace_manifest() → fs.write (aipm.toml)
              ├─→ scaffold_marketplace()
              │     ├─→ fs.create_dir_all(.ai/)
              │     ├─→ fs.write(.ai/.gitignore)
              │     ├─→ generate_marketplace_json() → fs.write(.ai/.claude-plugin/marketplace.json)
              │     └─→ [unless no_starter]
              │           ├─→ fs.write(starter-aipm-plugin/skills/.../SKILL.md)
              │           ├─→ generate_scaffold_script() → fs.write(.../scaffold-plugin.ts) ← TO REPLACE (#361)
              │           ├─→ fs.write(agents/marketplace-scanner.md)
              │           ├─→ fs.write(hooks/hooks.json)
              │           ├─→ generate_starter_manifest() → fs.write(aipm.toml)
              │           └─→ generate_plugin_json() → fs.write(.claude-plugin/plugin.json)  ← BROKEN (#356)
              └─→ adaptors (claude)::apply() → create/merge .claude/settings.json

aipm-pack init
  └─→ wizard_tty::resolve() [aipm-pack/wizard_tty.rs]
        └─→ libaipm::init::init() [init.rs:57]
              ├─→ is_valid_package_name() ← DUPLICATE of manifest/validate.rs
              ├─→ create_directory_layout()
              │     ├─→ fs.create_dir_all(skills/ or agents/ or ...)
              │     └─→ create_skill_template() → fs.write(skills/default/SKILL.md)
              └─→ generate_manifest() → fs.write(aipm.toml) ← FORMAT! APPROACH

aipm migrate
  └─→ libaipm::migrate::migrate() [migrate/mod.rs]
        ├─→ [per artifact] emitter::emit_plugin()
        │     ├─→ fs.create_dir_all(plugin_dir/.claude-plugin/)
        │     ├─→ [per type] type-specific subdirs
        │     ├─→ generate_plugin_manifest() [PluginToml struct + serde] ← SEPARATE IMPL
        │     └─→ generate_plugin_json_multi() ← SEPARATE IMPL
        └─→ registrar::register_plugins() → read/modify/write marketplace.json
```

### Current Filesystem Abstraction Boundary

```
Uses Fs trait (mockable):           Bypasses Fs trait (std::fs direct):
  workspace_init/mod.rs               lockfile/mod.rs
  workspace_init/adaptors/claude.rs   linker/link_state.rs
  init.rs                             linker/gitignore.rs
  migrate/emitter.rs                  installer/manifest_editor.rs
  migrate/registrar.rs                installer/pipeline.rs
  migrate/skill_common.rs             workspace/mod.rs
  lint/rules/*.rs                     manifest/mod.rs
  discovery.rs (ignore crate)         locked_file.rs
                                      cache.rs
                                      aipm/src/main.rs (installed registry)
```

### Lint Rule Architecture

The `Rule` trait has two dispatch paths:
- **Unified pipeline** (`check_file`): `lint/mod.rs` → `discover_features()` → per-file `check_file()` → `apply_rule_diagnostics()` → `Outcome`
- **Legacy path** (`check`): `lint/mod.rs` → `discover_features()` → per-source-dir `check()` → scan.rs re-walks `.ai/`

All 18 rules implement both. Migration to unified pipeline only is incomplete.

---

## Historical Context (from research/)

- [`research/docs/2026-03-19-init-tool-adaptor-refactor.md`](../docs/2026-03-19-init-tool-adaptor-refactor.md) — Original ToolAdaptor trait design document; introduced Claude-only adaptor as the extensibility point for engine-specific init. Directly relevant to #363 (make actions need the same pattern extended to post-init operations).
- [`research/docs/2026-03-31-110-aipm-lint-architecture-research.md`](../docs/2026-03-31-110-aipm-lint-architecture-research.md) — Comprehensive lint architecture research including the two-tier discovery design and rule boilerplate. Relevant to B10 findings.
- [`research/docs/2026-03-20-30-better-default-plugin.md`](../docs/2026-03-20-30-better-default-plugin.md) — Prior research on improving the default plugin created by `aipm init`. Directly relevant to #356.
- [`research/docs/2026-03-20-scaffold-plugin-ts-missing-features.md`](../docs/2026-03-20-scaffold-plugin-ts-missing-features.md) — Documents missing features in the TypeScript scaffold script (autoenable, marketplace registration). Confirms B4/B5 duplications were known issues.
- [`research/docs/2026-04-06-plugin-system-feature-parity-analysis.md`](../docs/2026-04-06-plugin-system-feature-parity-analysis.md) — Plugin system feature parity analysis. Useful for understanding what capabilities `aipm make` needs to match in other tools.
- [`research/docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md`](../docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md) — Documents the init/migrate TOML generation split. Confirms B1 was known.

---

## Related Research

- [`research/tickets/2026-03-28-110-aipm-lint.md`](../tickets/2026-03-28-110-aipm-lint.md) — Original lint ticket with architectural decisions
- [`research/docs/2026-04-07-lint-rules-287-288-289-290.md`](../docs/2026-04-07-lint-rules-287-288-289-290.md) — New lint rules for marketplace/plugin.json validation (adds to B10 boilerplate)

---

## Open Questions

1. **Should `Manifest` derive `Serialize`?** Adding `#[serde(skip_serializing_if = "Option::is_none")]` throughout `manifest/types.rs` and deriving `Serialize` would eliminate the need for `PluginToml`/`PluginPackage` in the emitter and the `format!()` paths in `init.rs` and `workspace_init/mod.rs`. Trade-off: TOML serialized from `Manifest` would include all optional fields as absent rather than omitted, and comment headers would be lost. `toml_edit` is already used in `manifest_editor.rs` for comment preservation.

2. **Should the `Fs` trait boundary be extended to cover `lockfile`, `linker`, and `installer`?** This would make those modules testable with mock filesystems. The cost is refactoring 10+ modules to accept `&dyn Fs` arguments.

3. **Should `wizard.rs`/`wizard_tty.rs` be moved to `libaipm`?** As `aipm make` adds more interactive paths, sharing the `PromptStep`, `PromptKind`, `PromptAnswer` types and `styled_render_config` in `libaipm` would prevent further divergence. The `inquire` dependency would need to move to `libaipm`.

4. **Is the legacy `check()` path ready to be removed?** If all 18 rules' `check()` implementations are considered functionally equivalent to their `check_file()` counterparts (which the two-tier architecture implies), removing `check()` would eliminate 300+ lines of boilerplate and the second directory walk. This requires verifying that `discover_features()` correctly classifies all files that the legacy scan picks up.

5. **Should `locate_json_key()` become a helper in `scan.rs` or a utility in `lint/rules/mod.rs`?** It currently lives in two identical copies in hook rules and is a natural candidate for consolidation before adding more hook-related rules.
