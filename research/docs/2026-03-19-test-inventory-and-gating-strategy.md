---
date: 2026-03-19
researcher: Claude Opus 4.6
git_commit: b74309c44b8d981cc823db377c4423e57907e8ca
branch: main
repository: aipm
topic: "Test inventory: wired vs pending BDD/E2E tests and strategy for disabling unimplemented scenarios"
tags: [research, testing, bdd, cucumber-rs, e2e, ci-cd, gating]
status: complete
last_updated: 2026-03-19
last_updated_by: Claude Opus 4.6
---

# Test Inventory & Gating Strategy

## Research Question

Complete breakdown of E2E and BDD tests: which are wired up and passing, which exist but have no step implementations (pending), and how to disable the pending ones so they don't block CI/CD while new features are developed.

## Summary

The repo has **233 BDD scenarios** across 20 feature files, but only **~39 scenarios** (in 4 feature files) have step implementations in `bdd.rs`. The remaining **~194 scenarios** across 16 feature files are **skipped** at runtime — they have no matching step definitions. Additionally, there are **80 unit tests** and **27 E2E tests** that are all fully implemented and passing.

The BDD harness (`crates/libaipm/tests/bdd.rs`) discovers ALL `.feature` files under `tests/features/` and runs them. Unimplemented scenarios are reported as skipped by cucumber-rs, which can cause CI noise or failures depending on configuration.

---

## Detailed Findings

### 1. Tests That Are Wired Up and Passing

#### 1.1 BDD Scenarios with Step Implementations (~39 scenarios in 4 files)

| Feature File | Scenarios | Status |
|---|---|---|
| `tests/features/manifest/init.feature` | 6 (+ 5 outline examples) | Passing — steps use `aipm-pack init` binary |
| `tests/features/manifest/validation.feature` | 9 | Passing — steps use `parse_and_validate()` |
| `tests/features/manifest/versioning.feature` | 5 (17 instances with outlines) | Passing — steps use `version::Requirement` |
| `tests/features/manifest/workspace-init.feature` | 19 | Passing — steps use `aipm init` binary |

**Step definitions in `bdd.rs`** cover these Gherkin phrases:

*Given:*
- `an empty directory {string}`
- `a directory {string} containing an {string}`
- `a plugin directory {string} with a valid manifest`
- `the manifest is missing the {string} field`
- `the manifest version is {string}`
- `the manifest declares a dependency {string} with version {string}`
- `the manifest declares a skill at {string}`
- `the file {string} does not exist`
- `the manifest has type {string}`
- `a manifest with version {string}`
- `a dependency with version requirement {string}`
- `the registry contains versions {string}, {string}, {string}`

*When:*
- `the manifest is validated`
- `the requirement is parsed`
- `dependencies are resolved`
- `the user runs {string} in {string}`
- `the user runs {string}`

*Then:*
- `the version is accepted` / `the version is rejected with {string}`
- `it matches version {string}` / `it does not match version {string}`
- `version {string} is selected` / `version {string} is not considered`
- `the command succeeds` / `the command fails with {string}`
- `no warnings are emitted`
- `a file {string} is created in {string}` / `a file {string} exists in {string}`
- `the manifest contains the directory name {string} as the package name`
- `the manifest contains a version of {string}`
- `the manifest contains an edition field`
- `the manifest contains the package name {string}`
- `the manifest contains the plugin type {string}`
- `a starter template for {string} is created`
- `the error message explains the naming rules`
- `all declared component paths are verified to exist`

#### 1.2 Unit Tests (80 tests, all passing)

| File | Test Count | Coverage Area |
|---|---|---|
| `crates/libaipm/src/lib.rs` | 1 | Version string |
| `crates/libaipm/src/init.rs` | 10 | Package init, naming, types |
| `crates/libaipm/src/workspace_init.rs` | 19 | Workspace + marketplace scaffolding |
| `crates/libaipm/src/version.rs` | 19 | Semver parsing, matching, selection |
| `crates/libaipm/src/manifest/mod.rs` | 24 | Manifest parsing, round-trips |
| `crates/libaipm/src/manifest/validate.rs` | 7 | Name/version validation |

#### 1.3 E2E Tests (27 tests, all passing)

| File | Test Count | Coverage Area |
|---|---|---|
| `crates/aipm/tests/init_e2e.rs` | 12 | `aipm init` binary behavior |
| `crates/aipm-pack/tests/init_e2e.rs` | 15 | `aipm-pack init` binary behavior |

---

### 2. BDD Scenarios That Exist but Have NO Step Implementations (~194 scenarios in 16 files)

These scenarios are discovered by the BDD harness but have no matching step definitions. They are reported as **skipped** at runtime.

#### dependencies/ (4 files, ~41 scenarios)

| Feature File | Scenarios | Topic |
|---|---|---|
| `tests/features/dependencies/resolution.feature` | 13 | Backtracking solver, version unification, overrides |
| `tests/features/dependencies/lockfile.feature` | 12 | Lockfile creation, reconciliation, `--locked` mode |
| `tests/features/dependencies/features.feature` | 6 | Optional features, additive unification |
| `tests/features/dependencies/patching.feature` | 10 | `aipm patch` / `patch-commit` workflow |

#### registry/ (6 files, ~83 scenarios)

| Feature File | Scenarios | Topic |
|---|---|---|
| `tests/features/registry/install.feature` | 23 | Full install flow, global store, lifecycle scripts |
| `tests/features/registry/publish.feature` | 17 | `aipm-pack pack` and `aipm-pack publish` |
| `tests/features/registry/security.feature` | 8 | Checksums, audit, auth |
| `tests/features/registry/yank.feature` | 5 | Yank/un-yank workflow |
| `tests/features/registry/link.feature` | 10 | `aipm link`/`unlink` local dev overrides |
| `tests/features/registry/local-and-registry.feature` | 19 | Symlink/junction, gitignore, vendoring |

#### monorepo/ (1 file, ~25 scenarios)

| Feature File | Scenarios | Topic |
|---|---|---|
| `tests/features/monorepo/orchestration.feature` | 25 | Workspace protocol, catalogs, filtering, Rush/Turborepo |

#### environment/ (1 file, ~11 scenarios)

| Feature File | Scenarios | Topic |
|---|---|---|
| `tests/features/environment/dependencies.feature` | 11 | `aipm doctor`, env vars, platform constraints |

#### guardrails/ (1 file, ~11 scenarios)

| Feature File | Scenarios | Topic |
|---|---|---|
| `tests/features/guardrails/quality.feature` | 11 | `aipm-pack lint`, quality scores |

#### reuse/ (1 file, ~11 scenarios)

| Feature File | Scenarios | Topic |
|---|---|---|
| `tests/features/reuse/compositional-reuse.feature` | 11 | Cross-type deps, composite packages |

#### portability/ (1 file, ~12 scenarios)

| Feature File | Scenarios | Topic |
|---|---|---|
| `tests/features/portability/cross-stack.feature` | 12 | Node.js/.NET/Python/Rust interop, offline mode |

---

### 3. How to Disable Pending Scenarios

The BDD harness in `bdd.rs:466-469` currently discovers ALL feature files:

```rust
fn main() {
    let features_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/features");
    futures::executor::block_on(AipmWorld::run(features_dir));
}
```

#### Option A: Point the harness at only implemented feature directories (simplest)

Change the `main()` function to specify individual feature file paths instead of the entire `tests/features/` directory:

```rust
fn main() {
    let features_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/features/manifest");
    futures::executor::block_on(AipmWorld::run(features_dir));
}
```

This runs only `manifest/` scenarios (init, validation, versioning, workspace-init) — the 4 files with complete step implementations.

#### Option B: Move unimplemented features to a `pending/` directory

```
tests/
  features/
    manifest/          ← keep (all wired up)
    pending/           ← move everything else here
      dependencies/
      registry/
      monorepo/
      environment/
      guardrails/
      reuse/
      portability/
```

The BDD harness continues pointing at `tests/features/` but never discovers `pending/` subdirectories (move them outside the scan path).

#### Option C: Use cucumber-rs tags to filter

Add `@wip` or `@pending` tags to unimplemented scenarios and configure the harness to skip them:

```rust
fn main() {
    let features_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/features");
    futures::executor::block_on(
        AipmWorld::cucumber()
            .with_default_cli()
            .filter_run(features_dir, |_, _, sc| !sc.tags.iter().any(|t| t == "pending"))
    );
}
```

Then tag each unimplemented feature file with `@pending` at the top.

---

## Code References

- `crates/libaipm/tests/bdd.rs:466-469` — BDD harness main function (feature directory path)
- `crates/libaipm/tests/bdd.rs:127-214` — Given step implementations
- `crates/libaipm/tests/bdd.rs:250-281` — When step implementations
- `crates/libaipm/tests/bdd.rs:287-460` — Then step implementations
- `crates/aipm/tests/init_e2e.rs` — 12 E2E tests for `aipm` binary
- `crates/aipm-pack/tests/init_e2e.rs` — 15 E2E tests for `aipm-pack` binary

## Architecture Documentation

The test architecture has three layers:
1. **Unit tests** — `#[cfg(test)]` modules inside `libaipm` source files (80 tests)
2. **E2E tests** — `assert_cmd`-based binary tests in `crates/*/tests/` (27 tests)
3. **BDD tests** — cucumber-rs harness in `crates/libaipm/tests/bdd.rs` scanning `tests/features/**/*.feature` (233 scenarios, ~39 implemented)

The BDD harness uses `clippy::allow` attributes (permitted in test files per `clippy.toml` exemptions) for `unwrap_used`, `expect_used`, `panic`, etc.

## Historical Context (from research/)

- `research/progress.txt` — Session 1 noted "245 scenarios, all skipped pending step impls"; Sessions 2-3 wired up manifest, workspace-init, and versioning steps
- `research/feature-list.json` — Features 1-7 marked `passes: true`; Features 8+ marked `passes: false` (correspond to the unimplemented BDD scenarios)

## Open Questions

1. Which option (A, B, or C) does the user prefer for disabling pending scenarios?
2. Should the pending feature files be preserved as-is for future implementation, or pruned to match only planned near-term work?
