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
