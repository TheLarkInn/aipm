# VS Code Extension (`vscode-aipm`)

The `vscode-aipm` extension brings `aipm lint` into your editor, providing real-time inline diagnostics, autocompletion for `aipm.toml` lint configuration, and hover documentation — all powered by the built-in `aipm lsp` language server.

## Requirements

- **VS Code** 1.85 or later
- **`aipm` binary** available on `PATH`, or configured via the `aipm.path` setting

The extension activates automatically when any workspace folder contains an `aipm.toml` file.

## Installation

> **Note:** The extension has not yet been published to the VS Code Marketplace. Install it from source using the [development setup](#development-setup) steps below.

Once published, install it from the VS Code Marketplace:

1. Open the **Extensions** panel (`Ctrl+Shift+X` / `Cmd+Shift+X`)
2. Search for **"aipm"**
3. Click **Install** on the **aipm — AI Package Manager** extension

## Configuration

The extension contributes two settings under the `aipm` namespace:

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `aipm.lint.enable` | boolean | `true` | Enable/disable lint diagnostics from the language server |
| `aipm.path` | string | `"aipm"` | Path to the `aipm` binary. Set this if `aipm` is not on your shell `PATH`. |

**Example workspace settings** (`.vscode/settings.json`):

```json
{
  "aipm.lint.enable": true,
  "aipm.path": "/home/yourname/.cargo/bin/aipm"
}
```

You can also set `AIPM_PATH` as an environment variable — it takes precedence over the `aipm.path` setting:

```bash
export AIPM_PATH=/path/to/aipm
code .
```

## Features

### Real-time lint diagnostics

When you open a supported file, the extension launches `aipm lsp` in the background and receives inline diagnostics from `aipm lint`. Violations appear as squiggly underlines in the editor and as entries in the **Problems** panel (`Ctrl+Shift+M` / `Cmd+Shift+M`).

Diagnostics are refreshed on file open and on every save, with a 300 ms debounce to avoid redundant relints on rapid saves.

### Supported file types

The language server attaches to these file patterns:

| File pattern | Purpose |
|---|---|
| `**/aipm.toml` | Workspace manifest — lint config completions and hover |
| `**/skills/SKILL.md` | Skill files — flat layout |
| `**/skills/*/SKILL.md` | Skill files — nested layout |
| `**/agents/*.md` | Agent files |
| `**/hooks/hooks.json` | Hook event configuration |
| `**/.ai/*/aipm.toml` | Plugin manifests under `.ai/` |
| `**/.ai/*/.claude-plugin/plugin.json` | Claude plugin JSON manifests |
| `**/.ai/.claude-plugin/marketplace.json` | Marketplace manifest |

### `aipm.toml` schema validation

The extension registers a JSON Schema for `aipm.toml` via the `tomlValidation` contribution point. If you have the [Even Better TOML](https://marketplace.visualstudio.com/items?itemName=tamasfe.even-better-toml) or [Taplo](https://marketplace.visualstudio.com/items?itemName=tamasfe.taplo) extension installed, you get:

- **Validation** — unknown keys and type mismatches are flagged inline
- **Autocomplete** — `Ctrl+Space` suggests valid fields and values

### Completions in `[workspace.lints]`

Inside the `[workspace.lints]` section of `aipm.toml`, the language server provides:

- **Rule ID completions** — pressing `Ctrl+Space` on a key position lists all known rule IDs (e.g., `skill/missing-name`, `hook/unknown-event`)
- **Severity value completions** — pressing `Ctrl+Space` after `=` suggests `"allow"`, `"warn"`, `"warning"`, `"error"`, or `"deny"`

### Hover documentation

Hovering over a rule ID in `[workspace.lints]` shows a popup with the rule's display name, default severity, help text, and a link to the full rule documentation.

## Troubleshooting

### "aipm language server stopped"

This error appears if the `aipm` binary cannot be found or crashes on startup. Steps to resolve:

1. Confirm `aipm` is installed: run `aipm --version` in your terminal.
2. If it's installed but not on `PATH`, set the `aipm.path` setting or the `AIPM_PATH` environment variable to the absolute path of the binary.
3. Click **Open Settings** in the error notification to jump directly to the setting.

### Diagnostics not appearing

- Ensure `aipm.lint.enable` is `true` (the default).
- Open the **Output** panel (`Ctrl+Shift+U`) and select **"aipm Language Server"** to view server logs.
- The extension only activates for workspaces containing `aipm.toml`. Check that this file exists at your project root.

### Schema validation not working

Schema validation for `aipm.toml` fields works best with the [**Even Better TOML**](https://marketplace.visualstudio.com/items?itemName=tamasfe.even-better-toml) extension installed. The `vscode-aipm` extension no longer lists it as a hard dependency, so VS Code will not auto-install it — you must install it manually from the Marketplace and reload VS Code.

---

## Development Setup

To run the extension locally from source (for contributors or testing a debug build):

### Prerequisites

1. **Install Node dependencies** (one-time setup):

   ```bash
   cd vscode-aipm
   npm install
   ```

2. **Build the `aipm` debug binary** (from the workspace root):

   ```bash
   cargo build -p aipm
   ```

### Launch configurations

Two launch configurations are available in `.vscode/launch.json`:

#### "Launch Extension (Extension Development Host)"

Launches an Extension Development Host with `vscode-aipm` loaded. The extension inherits your shell's `PATH`, so either:

- Add `./target/debug` to your shell `PATH` before opening VS Code, **or**
- Open a fixture folder in the host and add `"aipm.path": "/absolute/path/to/target/debug/aipm"` to that workspace's `settings.json`

**Pre-launch task:** `compile-extension` — compiles TypeScript only.

#### "Launch Extension with Fixture Folder"

Same as above, but automatically opens `fixtures/extension-test/` in the Extension Development Host and injects `AIPM_PATH` pointing at `./target/debug/aipm`. No PATH changes needed.

**Pre-launch task:** `build-and-compile` — runs `cargo build -p aipm` and `npm run compile` in parallel.

> **Tip:** On first use, run the **Install Extension Dependencies** task (`Ctrl+Shift+P` → **Tasks: Run Task** → `install-extension-deps`) to install `vscode-aipm`'s npm packages before launching.

### Available VS Code tasks

| Task label | What it does |
|---|---|
| `compile-extension` | Compiles TypeScript to `vscode-aipm/out/` (one-shot) |
| `watch-extension` | Compiles TypeScript in watch mode (background) |
| `build-aipm-debug` | Runs `cargo build -p aipm` |
| `install-extension-deps` | Runs `npm install` in `vscode-aipm/` |
| `build-and-compile` | Runs `build-aipm-debug` + `compile-extension` in parallel |

### Iterating on the extension

1. Press `F5` (or **Run → Start Debugging**) with the **"Launch Extension with Fixture Folder"** configuration selected.
2. A new Extension Development Host window opens with the `fixtures/extension-test/` folder.
3. Edit files in the fixture folder to trigger lint diagnostics.
4. To reload extension changes without restarting: `Ctrl+Shift+P` → **Developer: Reload Window** in the host window.

---

See also:
- [`aipm lsp`](../../README.md#aipm-lsp) — the language server subcommand
- [`docs/guides/lint.md`](./lint.md) — `aipm lint` CLI reference
- [`docs/guides/configuring-lint.md`](./configuring-lint.md) — rule severity overrides and ignore paths
- [`specs/2026-04-10-vscode-aipm-lint-integration.md`](../../specs/2026-04-10-vscode-aipm-lint-integration.md) — technical design document
