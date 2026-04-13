# Uninstalling Plugins

Remove a plugin from your project or the global registry with `aipm uninstall`.

## Project-level uninstall

```bash
aipm uninstall <package>
```

Removes the named plugin from the current project: the directory link under `.ai/`, the `.aipm/links.toml` entry, and the `.ai/.gitignore` entry are all cleaned up.

| Flag | Description |
|------|-------------|
| `--dir <DIR>` | Project directory (default: `.`) |

### Examples

```bash
# Remove by name
aipm uninstall my-plugin

# Remove in a specific project directory
aipm uninstall my-plugin --dir /path/to/project
```

## Global uninstall

```bash
aipm uninstall --global <package>
```

Removes the plugin from the global registry (`~/.aipm/installed.json`). Pass `--engine` to limit the removal to a single AI tool instead of all engines.

| Flag | Description |
|------|-------------|
| `--global` | Remove from the global registry |
| `--engine <ENGINE>` | Remove from one engine only (e.g. `claude`, `copilot`) |
| `--dir <DIR>` | Ignored when `--global` is set |

### Examples

```bash
# Full removal from all engines
aipm uninstall --global my-plugin

# Remove only from the Claude engine
aipm uninstall --global --engine claude my-plugin

# Use a fully qualified spec
aipm uninstall --global market:my-plugin@community
```

## Relationship to `aipm unlink`

For project-local plugins, `aipm uninstall <package>` and `aipm unlink <package>` perform the same operation. Use whichever matches your mental model:

- **`aipm uninstall`** — "I no longer want this plugin in my project."
- **`aipm unlink`** — "I'm done developing against this local override."

See [`docs/guides/local-development.md`](./local-development.md) for more on the link/unlink workflow.

---

See also: [`aipm uninstall`](../../README.md#aipm-uninstall), [`docs/guides/global-plugins.md`](./global-plugins.md), [`docs/guides/local-development.md`](./local-development.md).
