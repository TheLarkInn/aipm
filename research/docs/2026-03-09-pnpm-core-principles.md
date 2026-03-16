# pnpm Core Architectural Principles

**Date**: 2026-03-09
**Purpose**: Reference document covering pnpm's design decisions and the specific features that differentiate it from npm and yarn

---

## 1. Content-Addressable Store

**Source**: [Motivation | pnpm](https://pnpm.io/motivation)

pnpm maintains a single global content-addressable store (typically `~/.pnpm-store`). Every file from every package is stored exactly once, indexed by its SHA-512 hash. Projects reference files via hard links — no data duplication on disk.

- If two packages (or versions) contain an identical file, it exists once
- When a new version changes 1 file out of 100, only that 1 new file is stored
- Real-world reports cite 70-80% disk savings vs npm
- Hard links are near-instantaneous filesystem metadata operations vs full file copies

---

## 2. Strict node_modules Structure

**Source**: [Symlinked node_modules structure | pnpm](https://pnpm.io/symlinked-node-modules-structure)

npm/yarn flatten all dependencies to top-level `node_modules`, creating **phantom dependencies** — code can `require()` packages it never declared. pnpm uses an isolated layout:

```
node_modules/
  .pnpm/                          # Virtual store (flat internally)
    foo@1.0.0/node_modules/
      foo/  → hard links to store
      bar/  → symlink to ../../bar@1.0.0/node_modules/bar
  foo/  → symlink to .pnpm/foo@1.0.0/node_modules/foo   # Direct dep only
```

Only declared direct dependencies appear at root. Transitive deps live in `.pnpm/` and are accessible only to packages that declare them.

Escape hatches: `shamefullyHoist: true` for legacy compat, `hoistPattern` for selective hoisting.

---

## 3. Performance

**Source**: [Benchmarks | pnpm](https://pnpm.io/benchmarks)

| Scenario | npm | pnpm | Speedup |
|----------|-----|------|---------|
| Clean install | 33.4s | 8.3s | ~4x |
| Warm install (all cached) | 1.3s | 744ms | ~2x |
| CI (lockfile only) | 10.9s | 5.2s | ~2x |

Three concurrent stages (resolve + fetch + link), hard links instead of copies, content-addressable caching.

---

## 4. Workspace Protocol

**Source**: [Workspaces | pnpm](https://pnpm.io/workspaces)

```json
{ "dependencies": { "my-lib": "workspace:^" } }
```

On publish, `workspace:^` → `^1.5.0` (actual version). No manual patching needed.

Configured via `pnpm-workspace.yaml`. `--filter` flag for targeted commands.

---

## 5. Side-Effects Cache

Caches results of lifecycle scripts (postinstall, node-gyp builds) in the global store. Subsequent installs skip compilation.

pnpm v10: lifecycle scripts from dependencies **blocked by default** — explicit allowlist required (`pnpm.onlyBuiltDependencies`).

---

## 6. Filtering

**Source**: [Filtering | pnpm](https://pnpm.io/filtering)

```bash
pnpm --filter "@scope/app" build          # By name
pnpm --filter "./packages/**" test        # By path
pnpm --filter "[origin/main]" test        # Changed since main
pnpm --filter "foo..." build              # foo + all its deps
pnpm --filter "...foo" test               # foo + all its dependents
pnpm --filter "!bar" lint                 # Exclude bar
```

---

## 7. Strictness and Safety

- `autoInstallPeers: true` (default since v8)
- Strict isolated node_modules prevents phantom deps
- v10 blocks lifecycle scripts by default (supply-chain security)
- `packageExtensions` to fix broken upstream peer declarations

---

## 8. Patching

**Source**: [pnpm patch | pnpm](https://pnpm.io/cli/patch)

```bash
pnpm patch express@4.18.1      # Extract to temp dir for editing
pnpm patch-commit /tmp/...     # Save as .patch file
pnpm patch-remove express@4.18.1
```

Patches stored as unified diff files in `patches/`, tracked in VCS. No need for third-party `patch-package`.

---

## 9. Overrides and Hooks

**Source**: [.pnpmfile.cjs | pnpm](https://pnpm.io/pnpmfile)

```yaml
overrides:
  foo: ^1.0.0                    # Force version globally
  "baz>express": ^4.18.0        # Override only when dep of baz
  qux: "npm:qux-fork@^1.0.0"   # Replace with fork
```

`.pnpmfile.cjs` hooks: `readPackage`, `afterAllResolved`, `beforePacking` for programmatic mutation.

---

## 10. Catalogs

**Source**: [Catalogs | pnpm](https://pnpm.io/catalogs)

```yaml
# pnpm-workspace.yaml
catalog:
  react: ^18.2.0
  typescript: ^5.3.0
```

```json
{ "dependencies": { "react": "catalog:" } }
```

Single source of truth for version ranges across all workspace packages. Auto-replaced on publish.

---

## 11. Lockfile Format

`pnpm-lock.yaml` (YAML, not JSON). Contains `importers` (per-package), `packages` (resolved metadata), `snapshots`. Incompatible with `package-lock.json` due to fundamentally different layout model. `pnpm import` migrates from npm/yarn lockfiles.

---

## 12. Aliases and Custom Registries

```bash
pnpm add lodash@npm:awesome-lodash    # Install fork as original name
pnpm add lodash1@npm:lodash@1         # Two versions side-by-side
```

Per-scope registries in `.npmrc`:
```ini
@mycompany:registry=https://npm.mycompany.com/
```

---

## Summary: Why People Choose pnpm

| Pain Point | npm/Yarn Problem | pnpm Solution |
|-----------|-----------------|---------------|
| Disk waste | Every project duplicates all deps | Content-addressable store + hard links |
| Phantom deps | Flat hoisting leaks undeclared deps | Strict isolated node_modules |
| Slow installs | Sequential stages; file copies | Concurrent stages; hard links |
| Monorepo versions | Inconsistent across packages | Catalogs + workspace: protocol |
| CI speed | Full re-installs every build | Side-effects cache; --filter |
| Dep patching | Requires third-party tool | Built-in pnpm patch |
| Supply-chain risk | postinstall runs by default | v10 blocks scripts by default |
| Override granularity | No path selectors | Dep-path overrides; .pnpmfile.cjs hooks |

## Sources

- [pnpm.io/motivation](https://pnpm.io/motivation)
- [pnpm.io/symlinked-node-modules-structure](https://pnpm.io/symlinked-node-modules-structure)
- [pnpm.io/benchmarks](https://pnpm.io/benchmarks)
- [pnpm.io/workspaces](https://pnpm.io/workspaces)
- [pnpm.io/filtering](https://pnpm.io/filtering)
- [pnpm.io/catalogs](https://pnpm.io/catalogs)
- [pnpm.io/cli/patch](https://pnpm.io/cli/patch)
- [pnpm.io/pnpmfile](https://pnpm.io/pnpmfile)
- [pnpm.io/aliases](https://pnpm.io/aliases)
