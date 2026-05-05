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
| `engines = ["copilot-cli"]` | Copilot only |
| `engines = ["claude", "copilot-cli"]` | Both Claude and Copilot |

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

### Tool compatibility lint

The `engines` declaration also drives the **`valid-tool-name`** lint rule. Each AI engine
exposes a different set of tools, and some tools are engine-exclusive. When your feature files
(agents, skills, hooks) reference tools in their frontmatter `tools` field, `aipm lint`
cross-checks that declaration against the engine API schema:

| Situation | Diagnostic |
|-----------|------------|
| Tool used, no `engines` declared | Warning — suggests declaring the engines that support the tool |
| Tool used, declared `engines` has no match | Error — the tool will not be available at runtime |
| Tool used, declared `engines` includes a supporting engine | Clean |

**Shared tools** (`bash`, `glob`, `grep`, `web_fetch`) are available on all engines and
never trigger this rule.

**Example — claude-exclusive tool:**

```markdown
---
name: worktree-agent
tools: Task, bash
---
```

`Task` is only available in `claude`. Without `engines = ["claude"]` in `aipm.toml` this
emits a warning; with `engines = ["copilot-cli"]` it is an error.

See [`docs/rules/valid-tool-name.md`](../rules/valid-tool-name.md) for the full reference.

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
