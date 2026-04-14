# Updating Plugins (`aipm update`)

Keep your installed plugins current without breaking deterministic builds.

## Quick start

```bash
# Update all dependencies to the latest version within their declared ranges
aipm update

# Update a single package
aipm update code-review

# Update in a specific project directory
aipm update --dir ./my-project
```

## The Cargo-model lockfile

`aipm` follows the same lockfile discipline as Cargo:

| Command | What it does to the lockfile |
|---------|------------------------------|
| `aipm install` | **Never** upgrades existing pins — only resolves new or changed entries |
| `aipm update [PACKAGE]` | Re-resolves to the latest version **within** the declared range; rewrites the lock |
| `aipm install --locked` | CI mode — aborts if the lockfile drifts from the manifest |

This means running `aipm install` in a checked-out repo with an existing `aipm.lock` is
always safe: you get exactly the versions that were locked, not whatever happens to be
latest on the registry today.

## Install vs update semantics

### `aipm install` — pin-preserving

```bash
# Suppose aipm.toml declares: code-review = "^1.0"
# aipm.lock currently pins code-review to 1.1.0
# Registry now offers 1.5.0

aipm install          # Still installs 1.1.0 — the lock is respected
```

Only new or _changed_ dependency entries are resolved; existing pins are left alone.

### `aipm update` — explicit resolution

```bash
aipm update code-review   # Resolves 1.5.0, rewrites aipm.lock
aipm install              # Installs 1.5.0 going forward
```

`update` does **not** ignore the declared version range. If `aipm.toml` says `"^1.0"`,
then `2.0.0` will never be selected even if it is the latest available version.

## Updating all packages at once

```bash
aipm update
```

Every dependency is re-resolved to the latest version that satisfies its range. The
lockfile is fully regenerated. Packages whose ranges prevent a newer version are left
at their current pin.

## Updating a single package

```bash
aipm update code-review
```

Only `code-review` is re-resolved. All other lockfile entries are preserved exactly as
they were — the same guarantee as `aipm install`'s minimal-reconciliation behaviour.

## Version ranges and update

```toml
# aipm.toml
[dependencies]
code-review = "^1.0"   # allows 1.x, blocks 2.x
formatter   = "~0.9"   # allows 0.9.x only
```

```bash
# Registry state: code-review 1.5.0, 2.0.0 available; formatter 0.9.8 available
aipm update
# code-review → 1.5.0  (2.0.0 is outside ^1.0)
# formatter   → 0.9.8  (within ~0.9)
```

To upgrade across a breaking version boundary, update the range in `aipm.toml` first,
then run `aipm update`:

```toml
# Bump the range to allow 2.x
code-review = "^2.0"
```

```bash
aipm update code-review   # now resolves 2.0.0
```

## CI mode: `aipm install --locked`

Use `--locked` in CI pipelines to guarantee reproducible builds and catch accidental
lockfile drift before code ships:

```bash
# Fails with "lockfile is out of date" if aipm.lock doesn't match aipm.toml
aipm install --locked
```

The typical workflow:

1. Developers run `aipm update` locally and commit the updated `aipm.lock`.
2. CI runs `aipm install --locked` — passes only when the committed lock matches.
3. Any manifest change that isn't reflected in the committed lockfile causes CI to fail
   with a clear error, prompting the developer to run `aipm update` and commit again.

Commit `aipm.lock` to version control so that CI and your teammates install identical
versions.

## Error conditions

| Error message | Cause | Fix |
|---------------|-------|-----|
| `lockfile is out of date` | `--locked` detected drift between manifest and lock | Run `aipm update` locally and commit the new `aipm.lock` |
| `no package named '<name>' in aipm.toml` | Named package not in the manifest | Check spelling; the package must be declared in `[dependencies]` |
| `no compatible version found for '<range>'` | Registry has no version satisfying the range | Widen the version range in `aipm.toml` or publish a newer version |

## See also

- [`aipm install`](../../README.md#aipm-install) — Install new packages and restore from lockfile
- [Installing from Marketplaces](install-marketplace-plugin.md) — Marketplace install workflow
- [Download Cache](cache-management.md) — Cache policies used during resolution and download
