# Engine and Platform Compatibility

Declare which AI tool engines and operating systems your plugin supports.

## Engine Compatibility

### Declaring Engines

In your plugin's `aipm.toml`:

```toml
[package]
name = "my-plugin"
version = "1.0.0"
engines = ["claude", "copilot-cli"]    # Optional; omit for all engines
```

| Value | Meaning |
|-------|---------|
| `engines` omitted | Universal — works with all engines |
| `engines = []` | Universal — works with all engines |
| `engines = ["claude"]` | Claude only |
| `engines = ["copilot-cli"]` | Copilot CLI only |
| `engines = ["claude", "copilot-cli"]` | Both Claude and Copilot CLI |

### Validation Behavior

When a plugin is installed, aipm validates engine compatibility:

1. **If `aipm.toml` exists**: checks the `engines` field against the target engine
2. **If no `aipm.toml`**: falls back to checking engine-specific marker files

### Engine Marker Files

| Engine | Required Marker File(s) |
|--------|------------------------|
| Claude | `.claude-plugin/plugin.json` |
| Copilot | Any of: `plugin.json`, `.github/plugin/plugin.json`, `.claude-plugin/plugin.json` |

### Forward Compatibility

Unknown engine names (e.g., from a newer schema) are preserved as-is. They won't match any current engine but will be stored and compared correctly.

### Tool-Engine Compatibility Lint

The [`valid-tool-name`](../rules/valid-tool-name.md) lint rule catches mismatches between the tools a plugin uses and the engines it targets. Some tools (such as `Task`, `Edit`, and `Read`) are exclusive to Claude Code; others (such as `browser_navigate` and `search_code`) are exclusive to Copilot CLI. If a plugin references an engine-exclusive tool without declaring the corresponding engine, `aipm lint` warns:

```
warning[valid-tool-name]: Tool 'Task' is exclusive to claude; consider declaring engines = ["claude"] in aipm.toml.
```

If the plugin *does* declare engines and the declared set does not include any engine that supports the tool, the diagnostic is promoted to an error.

See the [valid-tool-name rule reference](../rules/valid-tool-name.md) for the complete tool lists and fix guidance.

## Platform Compatibility

### Declaring Platforms

In your plugin's `aipm.toml`:

```toml
[environment]
platforms = ["windows", "linux", "macos"]    # Optional; omit for all platforms
```

| Value | Meaning |
|-------|---------|
| `platforms` omitted | Universal — works on all platforms |
| `platforms = []` | Universal — works on all platforms |
| `platforms = ["windows"]` | Windows only |
| `platforms = ["linux", "macos"]` | Linux and macOS only |

### Checking Behavior

At install time, aipm checks if the current OS is in the declared platform list:

- **Universal**: No platforms declared → always compatible
- **Compatible**: Current OS is in the list → install proceeds
- **Incompatible**: Current OS is not in the list → **warning** emitted (non-blocking)

Platform incompatibility is a warning, not an error, because the plugin may still partially work or be used for development purposes.

### Supported Platforms

| Value | Matches |
|-------|---------|
| `"windows"` | Any Windows variant |
| `"linux"` | Any Linux variant |
| `"macos"` | Any macOS variant |

Unknown platform values (e.g., `"freebsd"`) are preserved for forward compatibility but won't match any current platform.

---

See also: [`Manifest format`](../../README.md#manifest-format-aipmtoml), [`docs/guides/creating-a-plugin.md`](./creating-a-plugin.md), [`docs/guides/install-git-plugin.md`](./install-git-plugin.md).
