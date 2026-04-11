# Download Cache Management

aipm caches downloaded plugins at `~/.aipm/cache/` to avoid redundant git clones.

## Cache Policies

| Policy | CLI Flag | Behavior |
|--------|----------|----------|
| **Auto** (default) | `--plugin-cache auto` | Use cache if fresh (within TTL); re-download if stale |
| **CacheOnly** | `--plugin-cache cache-only` | Use cache only; fail if not cached |
| **SkipCache** | `--plugin-cache skip` | Never read or write cache |
| **ForceRefresh** | `--plugin-cache force-refresh` | Always re-download; update cache |
| **CacheNoRefresh** | `--plugin-cache no-refresh` | Use cache if present regardless of age; fetch only if missing |

## Usage

```bash
# Default (Auto) — caches for 24 hours
aipm install github:org/repo:plugin@main

# Force fresh download
aipm install --plugin-cache force-refresh github:org/repo:plugin@main

# Offline / air-gapped (fail if not cached)
aipm install --plugin-cache cache-only github:org/repo:plugin@main

# Reproducible builds (ignore staleness)
aipm install --plugin-cache no-refresh github:org/repo:plugin@main
```

## Cache Location

```
~/.aipm/cache/
  cache_index.json       # JSON index tracking all entries
  entries/
    <uuid1>/             # Cached plugin content (directory)
    <uuid2>/
    ...
```

## Defaults

- **TTL**: 24 hours (auto policy considers entries stale after this)
- **GC threshold**: 30 days (entries not accessed in 30 days are eligible for cleanup)
- **Max files per plugin**: 500 (safety limit)

## Per-Entry TTL

Individual plugins can have custom TTL values. These are typically set by the installed registry for globally installed plugins.

## Garbage Collection

GC runs automatically after cache-using operations:

- **Stale entries**: Removed if `last_accessed` is older than 30 days AND not marked as "installed"
- **Installed entries**: Never GC'd (exempt because they're globally installed)
- **Unreferenced directories**: Cleaned up if older than 30 days and not tracked in the index
- **Young directories**: Preserved (may belong to a concurrent `put()` not yet indexed)

## Workflow Recommendations

| Workflow | Recommended Policy |
|----------|-------------------|
| Development (fast iteration) | `auto` (default) |
| CI/CD (reproducible) | `no-refresh` or `cache-only` |
| Air-gapped environments | `cache-only` (pre-populate cache) |
| Debugging plugin issues | `force-refresh` or `skip` |

---

See also: [`aipm install`](../../README.md#aipm-install), [`docs/guides/install-git-plugin.md`](./install-git-plugin.md), [`docs/guides/install-marketplace-plugin.md`](./install-marketplace-plugin.md), [`docs/guides/global-plugins.md`](./global-plugins.md), [`docs/guides/source-security.md`](./source-security.md).
