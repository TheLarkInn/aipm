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

### Convention Files

Each engine scans for specific convention files that carry project-level instructions. Understanding these helps you author files that are picked up correctly at runtime.

| File | Engine(s) | Default location(s) |
|------|-----------|---------------------|
| `CLAUDE.md` | Claude, Copilot | repo root, `.claude/` |
| `copilot-instructions.md` | Copilot | `.github/` |
| `AGENTS.md` | Copilot | repo root |
| `GEMINI.md` | Copilot | repo root |

> **Multi-engine repos**: Copilot reads `CLAUDE.md` and `AGENTS.md` alongside its own `copilot-instructions.md`. This means a single repo can carry instructions for multiple AI engines without duplication.

### Settings Paths

Each engine reads runtime configuration from specific paths:

| Engine | Settings paths |
|--------|----------------|
| Claude | `.claude/settings.json`, `.claude/settings.local.json` |
| Copilot | `.github/copilot/settings.json`, `.github/copilot/settings.local.json`, `.claude/settings.json`, `.claude/settings.local.json`, `~/.copilot/mcp-config.json` |

The `*.local.json` variants are user-local overrides that should be excluded from version control (add them to `.gitignore`).

### Forward Compatibility

Unknown engine names (e.g., from a newer schema) are preserved as-is. They won't match any current engine but will be stored and compared correctly.

## Tool Availability

Not all tools are available on every engine. Writing agent files that reference engine-exclusive tools will cause runtime errors when the plugin runs on the wrong engine.

### Shared Tools (all engines)

These tools work identically on both Claude and Copilot:

| Tool | Description |
|------|-------------|
| `bash` | Execute shell commands |
| `glob` | Glob pattern file search |
| `grep` | Search file contents |
| `web_fetch` | Fetch a web URL |

### Claude-exclusive Tools

These tools are only available when the plugin runs under Claude:

| Tool | Notes |
|------|-------|
| `Task` / `Agent` | Spawn sub-agent; supports `isolation:worktree` |
| `Edit` / `FileEdit` | Edit file content |
| `Read` / `FileRead` | Read file (text, image, notebook, pdf) |
| `Write` / `FileWrite` | Write file content |
| `WebSearch` | Web search |
| `TodoWrite` | Write todo items |
| `mcp`, `list_mcp_resources`, `read_mcp_resource` | MCP tool invocation |
| `notebook_edit` | Edit Jupyter notebook cells |
| `ask_user_question` | Ask user a question interactively |
| `enter_worktree`, `exit_worktree` | Git worktree isolation |
| `exit_plan_mode`, `task_output`, `task_stop` | Task lifecycle controls |

### Copilot-exclusive Tools

These tools are only available when the plugin runs under GitHub Copilot:

| Tool | Notes |
|------|-------|
| `get_file_contents`, `git_apply_patch` | File and patch operations |
| GitHub API tools | `get_pull_request`, `list_issues`, `create_issue`, etc. |
| `browser_navigate`, `browser_click`, … | Browser automation |
| Azure/cloud tools | `cosmos`, `keyvault`, `storage`, etc. |
| `store_memory` | Persist facts across turns |
| `semantic_issues_search` | Semantic search over issues |
| `sql` | Execute SQL queries |
| `report_intent`, `get_current_time`, `convert_time` | Utility tools |

### Restricting Tool Usage

If your agent uses engine-exclusive tools, declare the target engine in `aipm.toml` to prevent installation on incompatible engines:

```toml
[package]
name = "my-github-agent"
engines = ["copilot"]   # uses GitHub API tools
```

A future `agent/valid-tool-name` lint rule will warn when an unrestricted plugin (no `engines` declared) references engine-exclusive tools in agent frontmatter.

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
