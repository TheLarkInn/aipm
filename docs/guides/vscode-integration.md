# VS Code Integration

`aipm` ships a built-in Language Server Protocol (LSP) server (`aipm lsp`) and a
companion VS Code extension (`vscode-aipm`) that bring lint diagnostics,
autocompletion, and hover documentation directly into your editor.

## What you get

| Feature | Covered files |
|---------|--------------|
| Inline lint diagnostics (errors & warnings) | `aipm.toml`, `SKILL.md`, `agents/*.md`, `hooks/hooks.json`, `plugin.json`, `marketplace.json` |
| Autocomplete for rule IDs in `[workspace.lints]` | `aipm.toml` |
| Autocomplete for severity values (`warn`, `error`, `allow`, …) | `aipm.toml` |
| Hover documentation for lint rules | `aipm.toml` |
| JSON Schema validation for `aipm.toml` (via Even Better TOML) | `aipm.toml` |

## Requirements

- VS Code **1.85** or newer
- The [`tamasfe.even-better-toml`](https://marketplace.visualstudio.com/items?itemName=tamasfe.even-better-toml) extension (listed as an extension dependency — VS Code installs it automatically)
- The `aipm` binary on your `PATH` (or a custom path configured via `aipm.path`)

## Installation

### From the VS Code Marketplace _(not yet published)_

Once published, install directly from the Extensions pane:

```
ext install thelarkin.vscode-aipm
```

### Build from source

```bash
# From the repository root
cd vscode-aipm
npm install
npm run compile
# Then use "Install from VSIX" in VS Code after packaging with `vsce package`
```

## Configuration

Open **Settings** (`Ctrl+,` / `Cmd+,`) and search for `aipm`.

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `aipm.lint.enable` | boolean | `true` | Enable/disable aipm lint diagnostics |
| `aipm.path` | string | `"aipm"` | Path to the `aipm` binary. Override if `aipm` is not on your PATH or you want to pin a specific version. |

You can also set these in `.vscode/settings.json`:

```json
{
  "aipm.lint.enable": true,
  "aipm.path": "/usr/local/bin/aipm"
}
```

The `AIPM_PATH` environment variable takes precedence over `aipm.path` when the
extension is launched.

## How it works

The extension launches `aipm lsp` as a child process using stdio transport. On
every file **open** or **save**, the server runs `aipm lint` for the containing
workspace, converts the results to LSP diagnostics, and publishes them back to VS
Code. A 300 ms debounce on save prevents excessive re-linting while you are actively
editing.

The server advertises two additional capabilities for `aipm.toml` files:

- **Completions** — rule IDs in key position; severity values (`warn`, `warning`,
  `error`, `deny`, `allow`) in value position inside `[workspace.lints]`.
- **Hover** — shows the rule's default severity and help text when the cursor is over
  a rule ID.

Workspace detection walks up from the open file's directory and stops at the first
directory that contains an `aipm.toml` or `.ai/` folder.

## `aipm lsp` command reference

```
aipm lsp
```

Starts the Language Server Protocol server over **stdio**. There are no flags; the
server is entirely driven by LSP messages from the client.

> **Note:** You rarely need to invoke `aipm lsp` manually. The VS Code extension
> manages the server lifecycle automatically.

## Troubleshooting

### No diagnostics appear

1. Confirm `aipm` is installed: `aipm --version`
2. Check the **Output** panel in VS Code → select **aipm Language Server** from the
   dropdown for server logs.
3. Ensure `aipm.lint.enable` is `true` in your settings.
4. Make sure the workspace contains an `aipm.toml` or `.ai/` directory so that the
   server can locate the project root.

### `aipm` binary not found

Set `aipm.path` in your workspace settings to the absolute path of the binary:

```json
{
  "aipm.path": "/home/you/.cargo/bin/aipm"
}
```

Or add the directory containing `aipm` to your shell's `PATH` and restart VS Code.

### Diagnostics are stale after editing `aipm.toml`

The server re-lints on save. Press `Ctrl+S` / `Cmd+S` to trigger a refresh.

## See also

- [Using `aipm lint`](./lint.md) — CLI flags, output formats, and CI integration
- [Configuring Lint](./configuring-lint.md) — rule severity overrides and path ignores
- [Lint rule reference](../rules/) — individual rule pages with fix guidance
