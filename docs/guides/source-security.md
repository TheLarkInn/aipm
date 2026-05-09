# Source Security and Allowlists

Control which plugin sources are trusted in your environment.

## Configuration

In `~/.aipm/config.toml`:

```toml
[security]
# Glob patterns for allowed sources (case-insensitive, * wildcards)
allowed_sources = [
    "github.com/my-org/*",
    "github.com/trusted-partner/*",
    "git.company.com/*",
]

# Set to true to reject non-allowed sources (default: false = warn only)
enforce_allowlist = false
```

## Enforcement

| Setting | Behavior |
|---------|----------|
| `enforce_allowlist = false` (default) | Non-allowed sources produce a **warning** but install proceeds |
| `enforce_allowlist = true` | Non-allowed sources are **rejected** |
| `AIPM_ENFORCE_ALLOWLIST=1` env var | Overrides config to enforce (useful in CI) |

## Always-Trusted Sources

- **Local sources** (`local:./path`): Always allowed regardless of allowlist
- **Registry sources** (`name@version`): Always allowed (trust the registry)

## Pattern Matching

Patterns use `*` as a wildcard matching any sequence of characters:

```toml
[security]
allowed_sources = [
    "github.com/my-org/*",           # Any repo under my-org
    "github.com/*/my-plugin",         # my-plugin from any org
    "git.company.com/*",              # Any repo on company git
]
```

Matching is **case-insensitive**: `GitHub.com/My-Org/*` matches `github.com/my-org/repo`.

## CI/CD Setup

```yaml
# GitHub Actions example
env:
  AIPM_ENFORCE_ALLOWLIST: "1"

steps:
  - run: aipm install  # Will fail for untrusted sources
```

## Path Traversal Protection

All plugin paths are automatically validated for security:

- `..` directory traversal is rejected
- URL-encoded traversal (`%2e%2e`) is rejected
- Absolute paths (`/etc/passwd`, `C:\...`) are rejected
- Null bytes are rejected

This protection is always active — no configuration needed.

## Registry Package Checksum Verification

When aipm downloads a package from a registry, it verifies the downloaded tarball's
SHA-512 checksum against the value recorded in the registry index (`cksum` field).
If the checksums do not match, the install is aborted with a checksum mismatch error.

After installation the SHA-512 digest is recorded in `aipm.lock`:

```toml
[[package]]
name = "my-plugin"
version = "1.2.0"
source = "registry+https://github.com/org/registry.git"
checksum = "sha512-3b4c..."
```

On subsequent `aipm install --locked` runs, the stored checksum is re-verified before
the cached copy is used, detecting any tampering of the local cache.

> **NuGet-style registries**: The `sha512-` prefix on the `cksum` field in registry
> index files follows the same convention used by Cargo and NuGet. aipm strips the
> prefix before comparing the raw 128-character hex digest.

## Lint Path Containment

The lint rules that resolve paths from PR-author-controlled configuration files
(such as `marketplace.json` source paths and `aipm.toml` imports) validate every
path before any filesystem read. Paths containing `..`, absolute roots (`/`, `C:\`),
or Windows drive/UNC prefixes are never followed. The exact response depends on the
rule:

- **[`marketplace/source-resolve`](../rules/marketplace/source-resolve.md)** reports
  an **error diagnostic** for each unsafe `source` path in `marketplace.json`,
  surfacing the violation directly to the developer.
- **[`marketplace/plugin-field-mismatch`](../rules/marketplace/plugin-field-mismatch.md)**,
  **[`plugin/broken-paths`](../rules/plugin/broken-paths.md)**, and
  **[`instructions/oversized`](../rules/instructions/oversized.md)** (via import
  resolution) **silently skip** entries with unsafe paths — field reconciliation and
  size counting are impossible without a valid path, so those rules defer to
  `marketplace/source-resolve` to surface the underlying problem.

This is a defense-in-depth layer that operates independently of the plugin-path
validation above. No configuration is required.

---

See also: [`aipm install`](../../README.md#aipm-install), [`docs/guides/install-git-plugin.md`](./install-git-plugin.md), [`docs/guides/install-marketplace-plugin.md`](./install-marketplace-plugin.md), [`docs/guides/cache-management.md`](./cache-management.md), [`docs/guides/global-plugins.md`](./global-plugins.md).
