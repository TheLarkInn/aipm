# NPM Core Architectural Principles

**Date**: 2026-03-09
**Purpose**: Reference document covering npm's design decisions and architectural principles

---

## 1. Registry Model

### Design Philosophy

The npm registry is a centralized, read-heavy repository built on CouchDB, serving as the canonical source of truth for JavaScript packages. It uses a RESTful API at `registry.npmjs.org` for all interactions: publishing, querying metadata, and downloading tarballs.

### Architecture

The registry evolved a separation-of-concerns architecture to handle scale:

- **SkimDB**: Stores attachment-free metadata documents (lightweight)
- **FullfatDB**: Re-attaches binary tarballs from object storage for clients that need them
- **Manta/Object Storage**: Stores the actual package tarballs separately from CouchDB documents

This split was a direct response to CouchDB's struggles with storing hundreds of thousands of package tarballs as document attachments, which made view generation and compaction prohibitively expensive.

### Package Naming

- Names must be URL-safe characters, no leading dots or underscores
- Names are globally unique within the public registry
- Typosquatting protections exist: names too similar to existing popular packages may be blocked

### Scoped Packages (@org/package)

Scopes are a namespace mechanism that solves the "global flat namespace" problem:

- Every npm user/org automatically gets a scope matching their name
- Scoped packages install to `node_modules/@myorg/packagename` (a subfolder, not flat)
- **Scopes can be mapped to different registries**: a scope has a many-to-one relationship with registries, enabling seamless mixing of public and private packages
- **Private by default**: scoped packages are private unless explicitly published with `--access public`
- Scopes allow identical unscoped names to coexist under different organizations

### Public vs Private Registries

- Private packages are always scoped
- Enterprise registries (npmE) act as "edge nodes" to the public registry, replicating selected parts
- Proxy registries (Verdaccio, Nexus) can sit between developers and the public registry for caching, governance, and availability

**Sources**:
- [npm Registry Docs](https://docs.npmjs.com/cli/v11/using-npm/registry/)
- [npm Blog: New Registry Architecture](https://blog.npmjs.org/post/75707294465/new-npm-registry-architecture.html)
- [npm Blog: Registry Roadmap](https://blog.npmjs.org/post/100099402720/registry-roadmap.html)
- [About Scopes - npm Docs](https://docs.npmjs.com/about-scopes/)
- [Scope - npm Docs](https://docs.npmjs.com/cli/v11/using-npm/scope/)
- [Package Name Guidelines - npm Docs](https://docs.npmjs.com/package-name-guidelines/)
- [npm Blog: New Package Moniker Rules](https://blog.npmjs.org/post/168978377570/new-package-moniker-rules.html)

---

## 2. Versioning (Semver)

### Design Philosophy

npm enforces Semantic Versioning (semver) as a social contract between package authors and consumers. The version string `MAJOR.MINOR.PATCH` encodes compatibility intent:

- **MAJOR**: Breaking changes
- **MINOR**: Backwards-compatible new features
- **PATCH**: Backwards-compatible bug fixes

### Range Operators and Their Design Rationale

| Operator | Example | Meaning | Design Intent |
|----------|---------|---------|---------------|
| `^` (caret) | `^1.2.3` | `>=1.2.3 <2.0.0` | Default for `npm install`. Allows minor+patch updates. The "left-most non-zero digit" rule handles 0.x specially |
| `~` (tilde) | `~1.2.3` | `>=1.2.3 <1.3.0` | Conservative: only patch updates |
| Hyphen | `1.2.3 - 2.3.4` | `>=1.2.3 <=2.3.4` | Inclusive range |
| X-Range | `1.2.x` | `>=1.2.0 <1.3.0` | Wildcard flexibility |

**Key design decision for caret (`^`)**: For 0.x versions, caret is extra-conservative (`^0.2.3` means `>=0.2.3 <0.3.0`) because pre-1.0 packages are expected to have frequent breaking changes at the minor level.

### Pre-release Tags

Pre-release versions (e.g., `1.0.0-alpha.1`) are **excluded from range matching by default**. The rationale: "prerelease versions frequently are updated very quickly, and contain many breaking changes that are (by the author's design) not yet fit for public consumption." A range like `>=1.0.0` will NOT match `2.0.0-beta.1`. You must explicitly include the prerelease tuple (e.g., `>=1.0.0-beta.1`) to opt in.

### Coercion

The `semver.coerce()` function converts loose strings to valid semver (`"v2"` becomes `"2.0.0"`), reflecting npm's pragmatic approach to real-world messiness.

**Sources**:
- [node-semver (GitHub)](https://github.com/npm/node-semver)
- [About Semantic Versioning - npm Docs](https://docs.npmjs.com/about-semantic-versioning/)
- [semver - npm Docs](https://docs.npmjs.com/cli/v6/using-npm/semver/)
- [semver package](https://www.npmjs.com/package/semver)

---

## 3. Dependency Resolution

### Algorithm: Arborist (npm v7+)

npm uses the Arborist library to build a logical dependency graph overlaid on a physical folder tree. The algorithm is **maximally naive deduplication with nested fallback**.

### Hoisting Strategy

The resolution follows a greedy lifting process:

1. For each dependency, starting from the leaves of the tree
2. npm tries to place the package at the **highest possible ancestor** `node_modules` directory
3. A version is placed at the root if all dependents' version ranges are satisfied by that single version
4. If two dependents require **incompatible ranges**, separate copies are nested in their respective `node_modules` directories

### Deduplication Principle

When installing, npm prefers the latest available version that can be reused by the most dependents. If a common version satisfies multiple consumers, it goes to the root `node_modules`. Otherwise, incompatible versions are nested.

### Phantom Dependencies (a known tradeoff)

Hoisting creates phantom dependencies: package A can `require('B')` even though A does not declare B as a dependency, simply because B was hoisted for another package. This is a fundamental tradeoff of the flat `node_modules` approach.

### Peer Dependencies

- npm v7+ **enforces peer dependency conflicts by default** and will error on unsatisfiable peer requirements
- `--legacy-peer-deps` reverts to npm v6 behavior (ignoring conflicts)
- Peer dependencies express a compatibility contract: "I work alongside this package but don't bundle it"

### Tree Modification Flags

Flags like `--prefer-dedupe`, `--legacy-peer-deps`, and `--global-style` alter tree construction, and their effects are captured in the lockfile to preserve intent.

**Sources**:
- [npm-dedupe Docs](https://docs.npmjs.com/cli/v11/commands/npm-dedupe/)
- [Dependency Resolution Algorithms in npm (Medium)](https://medium.com/@aashvijariwala/dependency-resolution-algorithms-in-npm-c9c8b7a3ebca)

---

## 4. Lockfiles (package-lock.json)

### Core Design Principle: Lock the Tree, Not Just Resolutions

This is npm's key differentiator from yarn.lock. The npm blog states: "the Yarn tree building contract is split between the `yarn.lock` file and the implementation of Yarn itself. The npm tree building contract is entirely specified by the `package-lock.json` file."

This means:
- **yarn.lock** locks *which version* resolves for each specifier, but different Yarn versions could produce different tree shapes
- **package-lock.json** locks the *entire tree structure*, making it implementation-independent

### What It Stores

- **Exact versions** of every transitive dependency
- **Resolved URLs** for each package tarball
- **Integrity hashes** (SRI format, e.g., `sha512-...`) for verifying downloaded tarballs
- **Tree structure** (which packages are nested where)
- **Package metadata** (reducing the need for registry requests)
- **User intent** from flags like `--prefer-dedupe` and `--legacy-peer-deps`

### Deterministic Installs via `npm ci`

`npm ci` provides true determinism:
- Deletes `node_modules` entirely before installing
- Installs strictly from the lockfile (no resolution)
- Aborts if `package.json` and `package-lock.json` are inconsistent
- Never writes to the lockfile

In contrast, `npm install` may update the lockfile if newer versions satisfy the ranges.

**Sources**:
- [package-lock.json - npm Docs](https://docs.npmjs.com/cli/v11/configuring-npm/package-lock-json/)
- [npm v7 Series: Why Keep package-lock.json?](https://blog.npmjs.org/post/621733939456933888/npm-v7-series-why-keep-package-lockjson.html)

---

## 5. Manifest File (package.json)

### Required Fields

Only two fields are strictly required:
- **`name`**: The package identifier (URL-safe, lowercase, max 214 characters)
- **`version`**: Must be valid semver, parseable by node-semver

### Dependency Categories (Design Rationale)

Each category reflects a different relationship and installation context:

| Category | Installed in Production | Purpose |
|----------|------------------------|---------|
| `dependencies` | Yes | Runtime requirements |
| `devDependencies` | No (skipped with `--omit=dev`) | Build tools, test frameworks, linters |
| `peerDependencies` | Host must provide | Compatibility contracts (plugins, frameworks) |
| `optionalDependencies` | Attempted, failure tolerated | Platform-specific or nice-to-have packages |
| `bundledDependencies` | Shipped inside tarball | Packages preserved exactly as-is during publish |

### Key Structural Fields

- **`main`**: CommonJS entry point (legacy)
- **`exports`**: Modern conditional exports map supporting ESM/CJS dual packages, subpath exports, and conditional resolution
- **`bin`**: Maps command names to executable files, enabling CLI tools
- **`scripts`**: Named commands runnable via `npm run <name>`
- **`engines`**: Declares compatible Node.js/npm versions (advisory by default, enforced with `engine-strict`)
- **`files`**: Allowlist of files to include in the published tarball

### Design Principle

The manifest serves dual purposes: it is both a **human-authored declaration of intent** (dependencies, scripts, metadata) and a **machine-readable contract** (entry points, exports, engines). This duality is why it uses JSON rather than a more expressive format.

**Sources**:
- [package.json - npm Docs](https://docs.npmjs.com/cli/v11/configuring-npm/package-json/)

---

## 6. Publish Flow

### Publishing Mechanics

1. `npm publish` packs the project (respecting `files` and `.npmignore`)
2. Generates a tarball
3. Uploads metadata and tarball to the registry
4. The registry indexes the new version

### Access Control

- **Unscoped packages**: public by default
- **Scoped packages**: private by default, require `--access public` to publish publicly
- **Teams and organizations**: granular read/write permissions per package

### Two-Factor Authentication (2FA)

npm enforces 2FA as a supply chain security measure with tiered policies:
1. **Default**: Require 2FA or a granular access token with "bypass 2FA" enabled
2. **Recommended (strict)**: Require 2FA interactively, disallow all tokens regardless of settings

### Unpublish Policy

- **Within 72 hours**: Full unpublish allowed (for packages with minimal downloads)
- **After 72 hours**: Cannot unpublish; must use `npm deprecate` instead
- This policy was adopted after the infamous "left-pad" incident

### Trusted Publishing (Modern)

CI/CD systems can use OpenID Connect (OIDC) for token-free publishing, eliminating long-lived secrets from pipelines.

**Sources**:
- [Requiring 2FA for Package Publishing - npm Docs](https://docs.npmjs.com/requiring-2fa-for-package-publishing-and-settings-modification/)
- [About Two-Factor Authentication - npm Docs](https://docs.npmjs.com/about-two-factor-authentication/)

---

## 7. Workspaces (Monorepo Support)

### Design Philosophy

Workspaces provide built-in monorepo support from within a single top-level root package. The design is intentionally minimal compared to tools like Nx or Turborepo.

### How It Works

- The root `package.json` declares `"workspaces": ["packages/*"]` (glob patterns)
- Each matched directory is treated as an independent package with its own `package.json`
- npm hoists common dependencies to the root `node_modules`, creating symlinks for workspace packages

### Dependency Management Principles

- **Hoisting**: If multiple workspace packages use lodash, only one copy exists at the root
- **Symlinks**: Workspace packages that depend on each other get symlinked, enabling local development without publishing
- **Convention**: `devDependencies` go in root `package.json`; `dependencies` and `peerDependencies` go in each package's own `package.json`

### Known Limitations

npm workspaces deliberately omit features needed for large monorepos:
- No task dependency graph (build ordering)
- No result caching
- No affected-package detection

These are left to dedicated tools (Nx, Turborepo, Lerna) that can layer on top of npm workspaces.

**Sources**:
- [npm Workspaces and Monorepos Guide (Medium)](https://leticia-mirelly.medium.com/a-comprehensive-guide-to-npm-workspaces-and-monorepos-ce0cdfe1c625)
- [npm Workspaces Monorepo Management (Earthly)](https://earthly.dev/blog/npm-workspaces-monorepo/)

---

## 8. Lifecycle Scripts

### Design: Convention-Based Hooks

npm scripts follow a naming convention where any script can have automatic `pre` and `post` variants:

```
premyscript -> myscript -> postmyscript
```

### Built-in Lifecycle Scripts (Execution Order)

**On `npm install`:**
1. `preinstall`
2. `install`
3. `postinstall`
4. `prepublish` (DEPRECATED)
5. `prepare` (runs after install, intended for build steps)

**On `npm publish`:**
1. `prepublishOnly` (runs ONLY before publish, not on install)
2. `prepack`
3. `postpack`
4. `publish`
5. `postpublish`

### Key Design Decisions

- **`prepare` vs `prepublish`**: `prepublish` was deprecated because it ran on both `npm install` and `npm publish`, causing confusion
- **`prepack`/`postpack`**: Run during tarball creation, useful for build steps
- **Security concern**: `postinstall` scripts from dependencies are a major attack vector

**Sources**:
- [Scripts - npm Docs](https://docs.npmjs.com/cli/v11/using-npm/scripts/)

---

## 9. Init / Scaffolding

### The create-* Convention

When you run `npm init foo` (or equivalently `npm create foo`):

1. npm transforms the name: `foo` becomes `create-foo`
2. It installs `create-foo` via `npm exec`
3. It runs the package's `bin` entry

This is a **convention-over-configuration** design: any package named `create-*` automatically becomes an initializer.

### Design Principle

npm does not ship with built-in templates or scaffolding. Instead, it provides a **delegation mechanism** that lets the ecosystem own the scaffolding experience. This keeps npm minimal while enabling unlimited project types.

**Sources**:
- [npm init Docs](https://docs.npmjs.com/cli/v11/commands/npm-init/)

---

## 10. Security

### npm audit

`npm audit` scans the dependency tree against the GitHub Advisory Database:
- Reports severity levels: low, moderate, high, critical
- Can auto-fix with `npm audit fix`

### Integrity Checking (SRI Hashes)

Every entry in `package-lock.json` includes an `integrity` field containing a Subresource Integrity hash (typically `sha512`). On install, npm verifies downloaded content against this hash.

### Supply Chain Security Measures

- **Lockfile enforcement**: `npm ci` ensures installations match exactly what was audited
- **Token security**: Tokens should be read-only, IP-restricted, and rotated regularly
- **Trusted publishing**: OIDC-based publishing eliminates long-lived tokens in CI/CD
- **Provenance**: npm supports package provenance (Sigstore signing) to verify build origins

### Security Design Principles

1. **Defense in depth**: Multiple layers (integrity hashes, audit, lockfiles, 2FA, provenance)
2. **Shift left**: `npm audit` runs automatically on `npm install` to surface issues early
3. **Determinism as security**: Lockfiles prevent silent dependency substitution
4. **Least privilege**: Granular tokens with minimal permissions and short lifetimes

**Sources**:
- [npm audit - npm Docs](https://docs.npmjs.com/cli/v11/commands/npm-audit/)
- [OWASP NPM Security Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/NPM_Security_Cheat_Sheet.html)

---

## Summary of Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| Centralized registry on CouchDB | Optimized for read-heavy workloads; separated tarballs from metadata for scale |
| Scoped packages | Solved global namespace conflicts; enabled private packages |
| Caret (`^`) as default range | Balances receiving updates with avoiding breakage; special 0.x handling |
| Lockfile locks tree shape, not just versions | Implementation-independent determinism |
| Hoisting with nested fallback | Disk efficiency at the cost of phantom dependencies |
| `prepare` replacing `prepublish` | Fixed confusing dual-trigger behavior |
| `create-*` convention | Ecosystem-owned scaffolding without npm shipping templates |
| 72-hour unpublish window | Supply chain stability after the left-pad incident |
| SRI integrity hashes in lockfile | Tamper detection at install time |
| 2FA required for publishing | Supply chain attack prevention |
