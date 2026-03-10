---
date: 2026-03-09 10:04:50 PDT
researcher: Claude Opus 4.6
git_commit: 9ed90fe83636e78e067b21f37d6fee72492dc0d7
branch: main
repository: aipm
topic: "BDD Feature Specification for AI Plugin Manager (AIPM)"
tags: [research, bdd, cucumber-rs, feature-files, package-manager, aipm]
status: complete
last_updated: 2026-03-09
last_updated_by: Claude Opus 4.6
last_updated_note: "All open questions resolved. Anonymized for open source. Registry=API. No marketplace interop. Schema→SchemaStore. Side-effects cache=global. Windows=junctions."
---

# Research: BDD Feature Specification for AIPM

## Research Question

Design a comprehensive set of cucumber-rs feature files describing the behavior of an AI-native package manager (AIPM). Feature files should cover NPM/Cargo core principles mapped to priority challenges: core package manager, compositional reuse, AI quality guardrails, monorepo orchestration, cross-stack portability, and environment dependencies.

## Summary

19 cucumber-rs `.feature` files were created across 7 domain areas, covering 220+ scenarios that describe the expected behavior of AIPM. The features are organized by priority (P0/P1) and domain concept, drawing from NPM, Cargo, and **pnpm** design principles while adapting them for AI-native plugin management.


## Feature File Inventory

### P0 — Core Package Manager

| File | Scenarios | Coverage |
|------|-----------|----------|
| `tests/features/manifest/init.feature` | 6 | Package initialization, scaffolding, naming conventions |
| `tests/features/manifest/validation.feature` | 8 | Manifest validation, component path verification |
| `tests/features/manifest/versioning.feature` | 5 | Semver enforcement, version ranges, pre-release handling |
| `tests/features/registry/publish.feature` | 18 | Pack format (.aipm archive), deterministic packing, publishing, immutability, scoped packages, file allowlists, binary separation |
| `tests/features/registry/link.feature` | 10 | Local dev overrides via aipm link, unlink, list --linked, CI-safe restore |
| `tests/features/registry/install.feature` | 22 | Installation, locked installs, integrity, content-addressable store, strict isolation, side-effects cache |
| `tests/features/registry/local-and-registry.feature` | 20 | Local + registry coexistence, symlinks, gitignore, vendoring, non-workspace mode |
| `tests/features/registry/yank.feature` | 5 | Yanking, un-yanking, deprecation |
| `tests/features/registry/security.feature` | 7 | Integrity checksums, audit, authentication |
| `tests/features/dependencies/resolution.feature` | 13 | Dependency resolution, unification, conflicts, overrides, version coexistence (Cargo model, no peer deps) |
| `tests/features/dependencies/lockfile.feature` | 12 | Lockfile creation, determinism, Cargo-model minimal reconciliation, CI-mode --locked, explicit update |

### P1 — Extended Capabilities

| File | Scenarios | Coverage |
|------|-----------|----------|
| `tests/features/dependencies/features.feature` | 6 | Optional features, default features, additive unification |
| `tests/features/dependencies/patching.feature` | 10 | Built-in dependency patching, patch lifecycle, safety checks |
| `tests/features/reuse/compositional-reuse.feature` | 9 | Shared skills, agents, MCP servers, hooks, composite packages |
| `tests/features/guardrails/quality.feature` | 10 | AI-assisted scaffolding, lint, publish gates, quality scores |
| `tests/features/monorepo/orchestration.feature` | 28 | Workspaces, orchestrators, workspace protocol, catalogs, filtering |
| `tests/features/portability/cross-stack.feature` | 10 | .NET/Python/Rust/Node compatibility, self-contained CLI, offline installs |
| `tests/features/environment/dependencies.feature` | 10 | System tool requirements, env vars, platform constraints, doctor command |
| `tests/features/registry/search.feature` | 5 | Package discovery, search by type, outdated checks |

## Architecture Decision: Local + Registry Plugin Coexistence

Repos today have local plugin directories (e.g. `claude-plugins/`) that Claude Code discovers by scanning. AIPM must integrate with this existing pattern.

### Confirmed Decisions

1. **Root `aipm.toml` always required** — explicit workspace/project declaration
2. **Global store + local `.aipm/` working set** — pnpm model: content-addressable global cache, project gets hard-linked working set in `.aipm/` (gitignored)
3. **Registry plugins symlinked into `claude-plugins/`** — Claude Code discovers them naturally. We cannot control Claude Code's discovery path, so we match it.
4. **Non-workspace single-package mode supported** — `aipm.toml` without `[workspace]` works for simple repos

### Directory Layout

```
repo/
  aipm.toml                          # workspace root (always required)
  aipm.lock                          # single lockfile for everything
  claude-plugins/                    # plugins directory (configurable via plugins_dir)
    my-local-plugin/                 # real directory (git tracked)
      aipm.toml                      # local plugin manifest, can declare registry deps
      skills/
    @company/                        # scope directory
      review-plugin/                 # symlink → .aipm/links/... (gitignored)
    .gitignore                       # managed by aipm: ignores symlinked installs
  .aipm/                             # gitignored entirely
    store/                           # content-addressable file store (or links to global)
    links/                           # resolved package directories (hard-linked from store)
```

### Plugin Modes

| Mode | Location | Git tracked? | Can have deps? |
|---|---|---|---|
| **Local** (authored in-repo) | `claude-plugins/my-tool/` | Yes | Yes (registry + workspace) |
| **Installed** (from registry) | Symlink in `claude-plugins/` → `.aipm/links/` | No (gitignored) | Already resolved |
| **Vendored** (forked from registry) | `claude-plugins/forked-tool/` | Yes | Yes (becomes workspace member) |

### Gitignore Strategy

`aipm install` adds symlink AND adds the name to `claude-plugins/.gitignore`. `aipm uninstall` removes both. Manual entries in the gitignore are preserved.

---

## Design Principles Applied

### From NPM
- **Scoped packages** (`@org/package`) for namespace isolation
- **Lockfile-driven determinism** (`aipm install --locked` mirrors `npm ci`)
- **Lifecycle hooks** for orchestrator integration
- **72-hour unpublish → yank-only model** for supply chain stability
- **`create-*` delegation pattern** adapted for `aipm init --type`
- **Security audit** against advisory databases
- **Integrity hashes** (SRI-style) in lockfiles

### From Cargo
- **TOML manifest** (`aipm.toml` instead of JSON)
- **Caret-default version ranges** (`^1.0` as default semver strategy)
- **Feature unification** (additive features across the dependency graph)
- **Workspace with shared lockfile** and dependency inheritance
- **Permanent archive** (no unpublishing, only yanking)
- **Convention-over-configuration** directory layout
- **Virtual workspace** support for pure multi-package repos

### From pnpm
- **Content-addressable store**: Global store with hard links eliminates file duplication across projects
- **Strict dependency isolation**: Only declared dependencies are accessible; phantom deps prevented by design
- **Side-effects cache**: Lifecycle script results (postinstall, native builds) cached and reused
- **Lifecycle script blocking**: Scripts from dependencies blocked by default; explicit allowlist required
- **Workspace protocol**: `workspace:^` references auto-replaced with real versions on publish
- **Catalogs**: Single source of truth for dependency version ranges across workspace members
- **Filtering**: Rich `--filter` flag with name globs, path patterns, git-diff selectors, dependency graph traversal
- **Built-in patching**: Native `aipm patch` command for modifying dependencies without forking
- **Dependency overrides**: Path-scoped overrides (`"skill-a>common-util" = "=2.1.0"`) for surgical version control
- **Auto-install peers**: Missing peer dependencies installed automatically with clear conflict warnings

### Security-First Design
- **Binary separation**: `aipm` (consumer, read-only) and `aipm-pack` (author, pack/publish/lint) are separate binaries. Consumers never need publish capability.
- **Transfer format**: `.aipm` archive (gzip-compressed tar). Deterministic packing (sorted files, zeroed timestamps). Max size enforced. Secrets excluded by default.
- **Normalized manifests**: Workspace and catalog references replaced with concrete versions on pack. Published manifests are self-contained.

### AI-Native Extensions (Beyond NPM/Cargo)
- **Component type system**: Skills, Agents, MCP servers, Hooks as first-class types
- **Cross-type dependency resolution**: A skill can depend on an MCP server
- **Environment dependency declarations**: Required tools, env vars, platform constraints
- **Quality guardrails**: Lint, quality scores, machine-readable errors for AI agents
- **Multi-format export**: Claude plugin, A2A agent cards
- **`aipm doctor`**: Cross-package environment requirement checker
- **Technology-stack agnosticism**: No runtime requirement (self-contained Rust binary)

## Mapping Challenges to Feature Files

| Priority | Challenge | Feature Files |
|----------|-----------|---------------|
| P0 | Package manager + registry model | `manifest/*`, `registry/*`, `dependencies/*` |
| P1 | Compositional reuse | `reuse/compositional-reuse.feature` |
| P1 | AI quality guardrails | `guardrails/quality.feature` |
| P1 | Monorepo orchestrator integration | `monorepo/orchestration.feature` |
| P1 | Cross-tech-stack portability | `portability/cross-stack.feature` |
| P1 | Environment dependencies | `environment/dependencies.feature` |

## Related Research

- `research/docs/2026-03-09-npm-core-principles.md` — NPM design decisions and architectural principles
- `research/docs/2026-03-09-cargo-core-principles.md` — Cargo design decisions and architectural principles
- `research/docs/2026-03-09-cucumber-rs-conventions.md` — cucumber-rs Gherkin syntax and project setup
- `research/docs/2026-03-09-pnpm-core-principles.md` — pnpm design decisions: store, isolation, catalogs, filtering, patching

## Open Questions

2. **Registry backend**: Self-hosted vs. existing infrastructure (crates.io-style index, or API-based like npm)?
3. **MCP server packaging**: Should aipm manage MCP server runtime dependencies (npm packages, Python packages) or only reference them?
4. **Plugin marketplace integration**: Should aipm interop with Claude Code's marketplace format, or define its own?
5. **Feature file step implementation order**: Recommend implementing P0 manifest/registry steps first, then dependency resolution, then P1 features.
