# Engine and Platform Compatibility

Declare which AI tool engines and operating systems your plugin supports.

## Engine Compatibility

### Declaring Engines

In your plugin's `aipm.toml`:

```toml
[package]
name = "my-plugin"
version = "1.0.0"
engines = ["claude", "copilot"]    # Optional; omit for all engines
```

| Value | Meaning |
|-------|---------|
| `engines` omitted | Universal — works with all engines |
| `engines = []` | Universal — works with all engines |
| `engines = ["claude"]` | Claude only |
| `engines = ["copilot"]` | Copilot only |
| `engines = ["claude", "copilot"]` | Both Claude and Copilot |

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

## Engine File Conventions

Each engine uses different folder layouts and configuration files. Understanding these helps you place plugin artifacts in the right locations.

### Folder Conventions

| Engine | Scanned directories |
|--------|---------------------|
| Claude | `.claude/`, `.claude/skills/`, `~/.claude/skills/` |
| Copilot | `.github/copilot/`, `.github/extensions/`, `.github/skills/`, `.github/agents/`, `.github/plugin/`, `.claude/`, `.claude/skills/`, `.claude-plugin/`, `.agents/`, `.agents/skills/`, `~/.copilot/`, `~/.copilot/extensions/`, `~/.copilot/skills/`, `~/.claude/skills/` |

### Settings Paths

| Engine | Settings files |
|--------|---------------|
| Claude | `.claude/settings.json`, `.claude/settings.local.json` |
| Copilot | `.github/copilot/settings.json`, `.github/copilot/settings.local.json`, `.claude/settings.json`, `.claude/settings.local.json`, `~/.copilot/mcp-config.json` |

### Convention Files

Engines pick up instructions from these well-known filenames:

| File | Engine(s) | Location |
|------|-----------|----------|
| `CLAUDE.md` | Both | project root or `.claude/` |
| `AGENTS.md` | Copilot | project root |
| `GEMINI.md` | Copilot | project root |
| `copilot-instructions.md` | Copilot | `.github/` |

## Cross-Engine Tool Compatibility

Plugin skills and hooks may reference engine tool names. Referencing an engine-exclusive tool in a universal plugin (no `engines` restriction) can cause silent failures on the unsupported engine.

### Shared tools (both engines)

These four tools work identically on Claude and Copilot:

| Tool | Description |
|------|-------------|
| `bash` | Execute shell commands |
| `glob` | File pattern matching |
| `grep` | File content search |
| `web_fetch` | Fetch a web URL |

### Claude-exclusive tools

`Task` / `Agent`, `Edit` / `FileEdit`, `Read` / `FileRead`, `Write` / `FileWrite`, `Glob`, `Grep`, `WebFetch`, `WebSearch`, `TodoWrite`, `mcp`, `list_mcp_resources`, `read_mcp_resource`, `notebook_edit`, `ask_user_question`, `enter_worktree`, `exit_worktree`, `exit_plan_mode`, `task_output`, `task_stop`

### Copilot-exclusive tools

`get_file_contents`, `git_apply_patch`, GitHub API tools (`get_pull_request`, `list_issues`, `search_code`, …), browser automation tools (`browser_navigate`, `browser_click`, …), Azure/cloud tools, `store_memory`, `semantic_issues_search`, `sequentialthinking`, `sql`, `report_intent`, `convert_time`, `get_current_time`

> The tool compatibility data above is derived from `research/engine-api-schema.json`, which is automatically updated by the weekly reverse-binary-analysis workflow. Check that file for the latest per-version tool catalog.

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

See also: [`Manifest format`](../../README.md#manifest-format-aipmtoml), [`docs/guides/creating-a-plugin.md`](./creating-a-plugin.md), [`docs/guides/install-git-plugin.md`](./install-git-plugin.md), [`research/engine-api-schema.json`](../../research/engine-api-schema.json).
