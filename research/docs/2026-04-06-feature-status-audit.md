---
date: 2026-04-06 22:45:00 UTC
researcher: Claude Opus 4.6
git_commit: 332da9bd3051a3249185e7591d733ced7573997a
branch: main
repository: aipm
topic: "Comprehensive feature status audit — what works, what's partial, what's stubbed"
tags: [research, codebase, audit, roadmap, feature-status, cli, library]
status: complete
last_updated: 2026-04-06
last_updated_by: Claude Opus 4.6
---

# Feature Status Audit

## Research Question

Review all docs, guides, README, fixtures, tests, and GitHub issues to produce a comprehensive assessment of what features actually work vs. what is stubbed or unimplemented. Inform roadmap realignment.

## Executive Summary

**aipm is further along than the README roadmap suggests.** The library layer (`libaipm`) has **27 fully-implemented modules** with **~1,600 unit tests** and **89%+ branch coverage**. Most CLI commands work end-to-end. The primary bottleneck is the **StubRegistry** in the `aipm` binary — the library has a real `GitRegistry` implementation, but the binary hardcodes a stub that always errors, blocking `install` and `update` for registry-sourced packages.

All 16 roadmap GitHub issues (#1–#15, #54) remain **open**, but the underlying library code for many of them is already implemented and tested at the unit level. The gap is in **wiring library code to CLI commands** and **BDD end-to-end coverage**.

---

## CLI Command Status Matrix

| Binary | Command | Status | What Works | What Doesn't |
|--------|---------|--------|------------|--------------|
| `aipm` | `init` | **WORKING** | Workspace scaffolding, `.ai/` marketplace, starter plugin, Claude/Copilot adaptors, interactive wizard, all flags | — |
| `aipm` | `install` (local) | **PARTIAL** | Lockfile validation (`--locked`), workspace dep resolution, manifest editing, link state, gitignore | Registry downloads (StubRegistry), path deps, `--registry` flag |
| `aipm` | `install --global` | **WORKING** | Metadata registry (`~/.aipm/installed.json`), engine scoping, cache policy | Does not deploy files — records specs only |
| `aipm` | `update` | **PARTIAL** | Manifest parsing, lockfile pin manipulation, workspace dep logic | Registry resolution/download (StubRegistry) |
| `aipm` | `link` | **WORKING** | Symlink creation, link state tracking, manifest validation | — |
| `aipm` | `unlink` | **WORKING** | Unlink, link state cleanup, gitignore cleanup | — |
| `aipm` | `uninstall` (local) | **WORKING** | Delegates to unlink pipeline | — |
| `aipm` | `uninstall --global` | **WORKING** | Per-engine removal, full uninstall, file-locked writes | — |
| `aipm` | `list` | **WORKING** | Local (lockfile + linked packages), global (`installed.json`) | — |
| `aipm` | `lint` | **WORKING** | 14 rules, 4 reporters (human/json/ci-github/ci-azure), config from `aipm.toml`, recursive discovery | — |
| `aipm` | `migrate` | **WORKING** | All 6 artifact types, recursive discovery, dry-run, `--destructive`, conflict resolution, external ref detection | — |
| `aipm-pack` | `init` | **WORKING** | Plugin scaffolding for all 6 types, interactive wizard, manifest generation | — |
| `aipm-pack` | `pack` | **NOT IMPLEMENTED** | — | Command does not exist (mentioned in doc comment only) |
| `aipm-pack` | `publish` | **NOT IMPLEMENTED** | — | Command does not exist |
| `aipm-pack` | `yank` | **NOT IMPLEMENTED** | — | Command does not exist |
| `aipm-pack` | `login` | **NOT IMPLEMENTED** | — | Command does not exist |

### Key Architectural Finding

The library at [`crates/libaipm/src/registry/git.rs`](https://github.com/TheLarkInn/aipm/blob/332da9bd3051a3249185e7591d733ced7573997a/crates/libaipm/src/registry/git.rs) has a real `GitRegistry` implementation that clones/fetches a package index and downloads tarballs via HTTP with SHA-512 verification. However, the `aipm` binary at [`crates/aipm/src/main.rs:215-239`](https://github.com/TheLarkInn/aipm/blob/332da9bd3051a3249185e7591d733ced7573997a/crates/aipm/src/main.rs#L215-L239) hardcodes a `StubRegistry` with the comment: *"placeholder until GitRegistry (git2/reqwest) is implemented"*. **Wiring the real registry to the CLI would unblock `install` and `update` for registry packages.**

---

## Library Module Status (all 27 modules)

Every module in `libaipm` is **COMPLETE** — fully implemented with unit tests, no stubs or TODOs in production code.

| Module | Tests | Description |
|--------|-------|-------------|
| manifest | 47 | Parse, validate, load `aipm.toml` |
| init | 24 | Plugin package scaffolding |
| workspace_init | 44 | Workspace + `.ai/` marketplace scaffolding |
| workspace | 13 | Workspace root discovery, member glob expansion |
| migrate | 438 | Full migration with 10 detectors, 6 artifact types |
| lint | 244 | 14 rules, config, diagnostics, 4 reporters |
| discovery | 26 | Gitignore-aware recursive plugin discovery |
| installer | 85 | Install/update pipeline, manifest editing |
| linker | 56 | Hard links, dir links, gitignore, security, state |
| lockfile | 31 | Deterministic lockfile, reconciliation |
| resolver | 69 | Backtracking constraint solver, overrides |
| store | 43 | Content-addressable store, SHA-512 |
| registry | 57 | Git-based registry, config routing, index |
| logging | 5 | Layered tracing subscriber |
| frontmatter | 38 | YAML front-matter parsing |
| fs | 11 | Trait-based filesystem abstraction |
| version | 22 | Semver parsing and comparison |
| spec | 111 | Multi-source plugin spec parsing |
| acquirer | 17 | Plugin acquisition pipeline |
| cache | 29 | Download cache with TTL policies |
| engine | 13 | Multi-engine validation |
| installed | 37 | Global installed plugin registry |
| locked_file | 7 | OS-level file locking |
| marketplace | 30 | Marketplace manifest parsing |
| path_security | 20 | Path traversal prevention |
| platform | 11 | Platform compatibility checking |
| security | 16 | Source allowlist enforcement |
| **TOTAL** | **~1,600** | |

---

## BDD Test Coverage Gap

Of **31 feature files** containing **324 scenarios**, only **4 files (57 scenarios)** have backing step definitions that run in CI:

| Feature File | Scenarios | Status |
|---|---|---|
| `manifest/init.feature` | 6 | **Running** |
| `manifest/versioning.feature` | 5 | **Running** |
| `manifest/workspace-init.feature` | 26 | **Running** |
| `manifest/migrate.feature` | 20 | **Running** |
| All other 27 files | 267 | **Spec-only** (no step definitions) |

The 267 unimplemented BDD scenarios serve as executable specifications for future features. Many of the features they describe (resolution, lockfile, linking, security) are already unit-tested in the library but lack end-to-end CLI integration tests.

---

## GitHub Issues Status

### Roadmap Issues — ALL 16 OPEN

| # | Category | Title | Library Status |
|---|----------|-------|----------------|
| 1 | Dependencies | Resolution | **Library complete** (69 resolver tests) |
| 2 | Dependencies | Lockfile | **Library complete** (31 lockfile tests) |
| 3 | Dependencies | Features | **Library complete** (feature flags in resolver) |
| 4 | Dependencies | Patching | **Not implemented** |
| 5 | Registry | Install | **Library partial** (85 installer tests, but StubRegistry in CLI) |
| 6 | Registry | Publish | **Not implemented** (no `pack`/`publish` commands) |
| 7 | Registry | Security | **Library partial** (security module exists, no `audit` command) |
| 8 | Registry | Yank | **Not implemented** (no `yank` command) |
| 9 | Registry | Link | **Library complete** (56 linker tests, CLI works) |
| 10 | Registry | Local & Registry Coexistence | **Library partial** (linking works, registry integration missing) |
| 11 | Monorepo | Orchestration | **Not implemented** (workspace loading exists, no orchestration) |
| 12 | Environment | Dependencies | **Not implemented** (manifest field exists, no `doctor` command) |
| 13 | Guardrails | Quality | **Library partial** (lint works, no publish-gate scoring) |
| 14 | Reuse | Compositional Reuse | **Library partial** (spec/acquirer/marketplace modules exist) |
| 15 | Portability | Cross-Stack | **Partial** (Claude + Copilot adaptors, no Cursor) |
| 54 | Environment | Host Versioning | **Not implemented** |

### Non-Roadmap Open Issues (11)

| # | Title | Category |
|---|-------|----------|
| 126 | aipm-pack init should auto-detect `.claude-plugin/plugin.json` | Enhancement |
| 127 | aipm-pack init can initialize every package in marketplace | Enhancement |
| 128 | aipm install inside plugin cwd shouldn't modify plugin manifest | Bug |
| 132 | CI: Analyze latest claude/copilot/opencode runtimes for API changes | CI |
| 183 | Lint: validate runtime requirements for plugins | New rule |
| 185 | Lint: new rule — prevent long instructions | New rule |
| 199 | [aw] No-Op Runs | Bot noise |
| 204 | Installer: Front page installer links broken | Bug |
| 205 | Lint: new AI reporter (auto-enable) | Enhancement |
| 206 | Lint: skill/oversized doesn't explain why 15000 char limit | Docs |
| 237 | [aw] Coverage Improver failed | Bot noise |

### Closed Feature Issues (shipped)

| # | Title |
|---|-------|
| 30 | Better default plugin |
| 74 | .gitignore update for tool-usage.log |
| 110 | Make `aipm lint` |
| 111 | `aipm migrate --destructive` flag |
| 123 | Migrate: handle all files |
| 129 | Workspace dependencies link |
| 170 | Fix installer README paths |
| 187 | Lint: recursive discovery for misplaced features |
| 189 | Implement verbosity levels |
| 198 | Lint display UX |
| 208 | Lint recursion in `.github` folder |

---

## Test Infrastructure Summary

| Category | Count |
|----------|-------|
| Fixture workspaces | 4 (33 files total) |
| BDD feature files | 31 (324 scenarios) |
| BDD scenarios running | 57 (4 feature files) |
| CLI integration tests | 7 (`assert_cmd`-based) |
| Inline `#[cfg(test)]` modules | 86 source files |
| Snapshot files (insta) | 31 `.snap` files |
| Total unit test functions | ~1,600 |
| Branch coverage | 89.07% |

---

## Feature Parity with Other Tools (from earlier research)

The project recently implemented 10 new modules to close feature gaps with other plugin management tools:

| Capability | Status | Module |
|------------|--------|--------|
| Download cache with TTL | **Implemented** | `cache.rs` (29 tests) |
| Multi-engine validation | **Implemented** | `engine.rs` (13 tests) |
| Path security (traversal) | **Implemented** | `path_security.rs` (20 tests) |
| Platform compatibility | **Implemented** | `platform.rs` (11 tests) |
| Multi-source plugin specs | **Implemented** | `spec.rs` (111 tests) |
| Plugin acquirer | **Implemented** | `acquirer.rs` (17 tests) |
| Marketplace manifests | **Implemented** | `marketplace.rs` (30 tests) |
| Installed plugin registry | **Implemented** | `installed.rs` (37 tests) |
| OS-level file locking | **Implemented** | `locked_file.rs` (7 tests) |
| Source allowlist | **Implemented** | `security.rs` (16 tests) |

---

## What Actually Works End-to-End (User Can Run Today)

1. **`aipm init`** — Scaffold a workspace with `.ai/` marketplace, tool configs, starter plugin
2. **`aipm migrate`** — Migrate existing `.claude/` configs into marketplace plugins (all artifact types)
3. **`aipm lint`** — Lint AI plugin configurations (14 rules, 4 output formats)
4. **`aipm-pack init`** — Scaffold a new plugin package with manifest
5. **`aipm link` / `unlink`** — Local development overrides via symlinks
6. **`aipm list`** — View installed/linked packages (local and global)
7. **`aipm install` (workspace deps only)** — Install workspace member-to-member dependencies
8. **`aipm install --global`** — Register global plugin metadata

## What Does NOT Work End-to-End

1. **`aipm install <pkg>` from registry** — StubRegistry blocks all registry operations
2. **`aipm update`** — Same StubRegistry blocker
3. **`aipm-pack pack`** — Command does not exist
4. **`aipm-pack publish`** — Command does not exist
5. **`aipm-pack yank`** — Command does not exist
6. **`aipm-pack login`** — Command does not exist
7. **`aipm doctor`** — Command does not exist (environment validation)
8. **`aipm audit`** — Command does not exist (security auditing)
9. **`aipm patch`** — Command does not exist (dependency patching)
10. **`aipm validate`** — Command does not exist (standalone manifest validation)

---

## Roadmap Realignment Recommendations

### Tier 1: Quick Wins (library code exists, just needs CLI wiring)

- **Wire GitRegistry to CLI** — Replace StubRegistry with real GitRegistry. Library is ready. Unblocks `install` and `update` for registry packages.
- **Enable `validation.feature` BDD scenarios** — Library has `parse_and_validate`, just needs a CLI entry point or updated BDD steps.
- **Close issue #9 (Link)** — `aipm link` / `unlink` is fully working. Issue can be closed.

### Tier 2: Moderate Effort (library partially ready)

- **Enable remaining BDD scenarios** — 267 scenarios across 27 files. Many test features already implemented in the library. Incrementally enable as step definitions are written.
- **Publish pipeline** — `aipm-pack pack` / `publish` / `yank` / `login` commands. No library code exists.
- **Environment validation** — `aipm doctor` command. Manifest field `[environment]` exists, needs checker logic.

### Tier 3: Significant Effort (new features)

- **Monorepo orchestration** — Workspace loading/discovery exists, but no filtering, ordering, or CI integration.
- **Dependency patching** — No library code exists.
- **Cross-stack portability** — Claude + Copilot adaptors exist. Cursor adaptor not started.

---

## Code References

- [`crates/aipm/src/main.rs`](https://github.com/TheLarkInn/aipm/blob/332da9bd3051a3249185e7591d733ced7573997a/crates/aipm/src/main.rs) — Consumer CLI dispatch
- [`crates/aipm-pack/src/main.rs`](https://github.com/TheLarkInn/aipm/blob/332da9bd3051a3249185e7591d733ced7573997a/crates/aipm-pack/src/main.rs) — Author CLI dispatch
- [`crates/libaipm/src/lib.rs`](https://github.com/TheLarkInn/aipm/blob/332da9bd3051a3249185e7591d733ced7573997a/crates/libaipm/src/lib.rs) — Library module exports
- [`crates/libaipm/tests/bdd.rs`](https://github.com/TheLarkInn/aipm/blob/332da9bd3051a3249185e7591d733ced7573997a/crates/libaipm/tests/bdd.rs) — BDD test runner (filters to 4 feature files)
- [`crates/libaipm/src/registry/git.rs`](https://github.com/TheLarkInn/aipm/blob/332da9bd3051a3249185e7591d733ced7573997a/crates/libaipm/src/registry/git.rs) — Real GitRegistry (not wired to CLI)

## Historical Context (from research/)

- `research/docs/2026-04-06-plugin-system-feature-parity-analysis.md` — Feature parity analysis with other plugin tools
- `research/docs/2026-03-26-install-update-link-lockfile-implementation.md` — Install/update/link/lockfile implementation readiness
- `research/docs/2026-03-31-110-aipm-lint-architecture-research.md` — Lint system architecture
- `research/docs/2026-03-23-aipm-migrate-command.md` — Migration command design
- `research/docs/2026-03-19-test-inventory-and-gating-strategy.md` — Test infrastructure strategy

## Open Questions

1. **When should the StubRegistry be replaced?** The library has a working GitRegistry with 23 tests. What's blocking the wiring — is it the git2/reqwest dependency concern noted in the stub comment, or is a registry server needed first?
2. **Should closed roadmap issues be batched?** Issues #1 (Resolution), #2 (Lockfile), #3 (Features), #9 (Link) have significant library implementations. Should they be partially closed or relabeled to track only remaining CLI/BDD work?
3. **What's the priority for aipm-pack commands?** `pack`, `publish`, `yank`, `login` have zero implementation. Are these blocked on the registry server existing?
4. **Is the installer link (#204) a release blocker?** The front-page installer links are reported broken.
