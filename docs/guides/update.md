# Updating Plugins

Keep your installed plugins current with `aipm update`.

## Update all plugins

```bash
aipm update
```

Resolves the latest version of every dependency within its declared version range in `aipm.toml`, fetches any newer packages, and rewrites `aipm.lock`. Plugins already at their maximum compatible version are left untouched.

## Update a single plugin

```bash
aipm update <package>
```

Only the named plugin is re-resolved. All other installed plugins remain at their current locked versions.

### Examples

```bash
# Update all plugins in the current project
aipm update

# Update only one plugin
aipm update my-plugin

# Update in a specific project directory
aipm update --dir /path/to/project

# Update a single plugin in a non-default directory
aipm update my-plugin --dir /path/to/project
```

## Options

| Flag | Description |
|------|-------------|
| `--dir <DIR>` | Project directory (default: `.`) |

## How `aipm update` differs from `aipm install`

| Behaviour | `aipm install` | `aipm update` |
|-----------|---------------|---------------|
| Resolves version | Exact spec given on the command line | Latest compatible with the range in `aipm.toml` |
| Adds new dependency | Yes | No |
| Rewrites lockfile | Yes (new entry) | Yes (bumps existing entries) |
| Works without a package name | No | Yes (updates all) |

Use `aipm install` to add a plugin for the first time or to pin a specific version.  Use `aipm update` to pull in improvements within your already-declared ranges.

## Understanding the output

```
Updated 2 package(s), 4 up-to-date, 0 removed
```

| Field | Meaning |
|-------|---------|
| `Updated N` | Packages fetched at a newer version |
| `up-to-date N` | Packages already at their latest compatible version |
| `removed N` | Packages removed because they were no longer reachable |

## Next steps

- **Pin a specific version** — edit `aipm.toml` and run `aipm install` to lock to an exact release.
- **Remove a plugin** — see [`docs/guides/uninstall.md`](./uninstall.md).
- **Inspect what is installed** — run `aipm list` (see [README](../../README.md#aipm-list)).

---

See also: [`aipm update`](../../README.md#aipm-update), [`docs/guides/install-marketplace-plugin.md`](./install-marketplace-plugin.md), [`docs/guides/uninstall.md`](./uninstall.md).
