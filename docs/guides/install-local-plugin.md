# Installing Plugins from Local Paths

Install plugins directly from your local filesystem without publishing to a registry.

## CLI Usage

```bash
# Install from a relative path
aipm install local:./path/to/plugin

# Install from nested directory
aipm install local:plugins/my-plugin
```

## Manifest Usage

In your `aipm.toml`:

```toml
[dependencies]
my-local-plugin = { path = "../my-plugin" }
```

Then run `aipm install` to install all dependencies.

## Project Layout

```
my-project/
  aipm.toml              # declares local dependency
  .aipm/
    plugins/
      my-plugin/         # linked plugin directory

../my-plugin/
  aipm.toml              # plugin manifest (or engine marker files)
  skills/
    my-skill/SKILL.md
```

## Validation

Local plugins are validated after copying:

1. **Primary check**: `aipm.toml` must exist at the plugin root. If present, the `engines` field is checked against the target engine.
2. **Fallback check**: If no `aipm.toml`, engine-specific marker files are required:
   - **Claude**: `.claude-plugin/plugin.json`
   - **Copilot**: any of `plugin.json`, `.github/plugin/plugin.json`, or `.claude-plugin/plugin.json`

## Lockfile Entry

Local dependencies are recorded in `aipm.lock` as:

```toml
[[package]]
name = "my-local-plugin"
version = "0.0.0"
source = "path+../my-plugin"
```

## Troubleshooting

| Error | Cause | Fix |
|-------|-------|-----|
| "Plugin directory does not exist" | The path doesn't point to a directory | Verify the relative path from your project root |
| "Plugin path is not a directory" | The path points to a file | Point to the directory containing the plugin |
| "Not a valid Claude plugin" | No `aipm.toml` or marker files found | Add an `aipm.toml` or `.claude-plugin/plugin.json` to the plugin |

---

See also: [`aipm install`](../../README.md#aipm-install), [`docs/guides/local-development.md`](./local-development.md), [`docs/guides/install-marketplace-plugin.md`](./install-marketplace-plugin.md), [`docs/guides/install-git-plugin.md`](./install-git-plugin.md), [`docs/guides/uninstall.md`](./uninstall.md), [`docs/guides/update.md`](./update.md) — lockfile semantics and version-range upgrades.
