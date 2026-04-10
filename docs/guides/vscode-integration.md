# VS Code Integration

`aipm` ships a Language Server Protocol (LSP) server and a companion VS Code
extension that bring live lint diagnostics, autocomplete, and hover documentation
into the editor — no terminal needed.

## Prerequisites

| Requirement | Version |
|---|---|
| VS Code | ≥ 1.85 |
| `aipm` binary | installed and on `PATH` (or configured via `aipm.path`) |
| [Even Better TOML](https://marketplace.visualstudio.com/items?itemName=tamasfe.even-better-toml) | any recent version |

The extension declares `tamasfe.even-better-toml` as a dependency; VS Code
installs it automatically when you install `vscode-aipm`.

---

## Installing the Extension

### From VSIX (manual install)

1. Build the extension from source (see [Development](#development) below) or
   download the `.vsix` artifact from the latest GitHub release.
2. In VS Code: **Extensions → ⋯ → Install from VSIX…** and select the file.

### From the Marketplace

The extension (`thelarkin.vscode-aipm`) will be available on the VS Code Marketplace
once published. Until then, install from VSIX as described above.

---

## Features

### Live Lint Diagnostics

When you open or save any supported file, the extension automatically runs
`aipm lint` on that file and publishes the results to the **Problems** panel
with squiggly underlines in the editor.

**Supported file types:**

| File pattern | Feature kind |
|---|---|
| `**/aipm.toml` | Workspace manifest |
| `**/skills/SKILL.md` | Skill (flat layout) |
| `**/skills/*/SKILL.md` | Skill (nested layout) |
| `**/agents/*.md` | Agent |
| `**/hooks/hooks.json` | Hook config |
| `**/.ai/*/aipm.toml` | Plugin manifest |
| `**/.ai/*/.claude-plugin/plugin.json` | Plugin JSON |
| `**/.ai/.claude-plugin/marketplace.json` | Marketplace manifest |

Diagnostics are debounced (300 ms after the last save) to avoid re-running lint
on every keystroke.

### Autocomplete for `aipm.toml`

When editing the `[workspace.lints]` section of `aipm.toml`, the extension
provides:

- **Rule ID completions** — all 17 built-in rule IDs (e.g. `skill/missing-name`)
- **Severity value completions** — `"allow"`, `"warn"`, `"warning"`, `"error"`, `"deny"`

Completions are context-aware: rule IDs appear only when the cursor is in a
rule-key position, and severity values appear only in a value position.

### Hover Documentation

Hover over any rule ID in `aipm.toml` to see its name, default severity, and a
link to the rule's documentation page.

### JSON Schema Validation for `aipm.toml`

The extension bundles a JSON Schema for the `[workspace.lints]` section and
registers it with Taplo via the `tomlValidation` contribution point. This provides:

- **Inline validation** of rule IDs (typos surface immediately as red squiggles)
- **Type checking** for severity values
- **Structure validation** for table-form rule config (the `level`/`ignore` syntax)

> **Note:** The schema covers only `[workspace.lints]`. Other sections
> (`[package]`, `[workspace]`, `[dependencies]`) are intentionally not included
> to keep the autocomplete surface small and focused.

The same schema is submitted to [SchemaStore.org](https://www.schemastore.org/json/)
so any editor using Taplo or Tombi picks it up automatically without installing the
extension.

---

## Configuration

| Setting | Type | Default | Description |
|---|---|---|---|
| `aipm.lint.enable` | boolean | `true` | Enable/disable live lint diagnostics |
| `aipm.path` | string | `"aipm"` | Path to the `aipm` binary |

### Disabling diagnostics

To turn off live linting while keeping autocomplete and schema validation:

```jsonc
// .vscode/settings.json
{
  "aipm.lint.enable": false
}
```

### Custom binary path

If `aipm` is not on `PATH`, point the extension to the exact binary:

```jsonc
// .vscode/settings.json
{
  "aipm.path": "/usr/local/bin/aipm"
}
```

---

## The `aipm lsp` Subcommand

The extension starts the language server by running:

```
aipm lsp
```

This launches the LSP server over **stdio**. The server advertises:

| Capability | Details |
|---|---|
| `textDocument/publishDiagnostics` | Published on file open and on save (300 ms debounce) |
| `textDocument/completion` | Rule IDs and severity values in `aipm.toml` |
| `textDocument/hover` | Rule documentation (name, severity, help URL) |

The server walks up the directory tree from each opened file to find the nearest
`aipm.toml` or `.ai/` marker in order to resolve workspace-relative paths during
linting.

> The `aipm lsp` subcommand is designed to be invoked by editors — running it
> directly in a terminal produces no visible output (it reads/writes JSON-RPC
> messages on stdin/stdout).

---

## Activation

The extension activates automatically in any workspace that contains an
`aipm.toml` file (`workspaceContains:**/aipm.toml`). No manual activation
is required.

If the `aipm` binary is not found when the server stops, the extension shows an
error notification:

> *aipm language server stopped. Check that the `aipm` binary is installed and
> accessible via PATH (or set `aipm.path`).*

---

## Development

### Building the extension from source

```bash
cd vscode-aipm
npm install
npm run compile        # one-shot compile
npm run watch          # incremental watch mode
```

The compiled extension output is written to `vscode-aipm/out/`.

### Debugging in the Extension Development Host

A VS Code launch profile is included in `.vscode/launch.json`:

| Profile | Description |
|---|---|
| **Launch Extension** | Opens a new VS Code window with the extension loaded |
| **Launch Extension with Fixture Folder** | Opens `fixtures/extension-test/` so you can see all 17 rules fire immediately |

Set the `AIPM_PATH` environment variable (or `aipm.path` setting) to point to
a debug build of the `aipm` binary:

```bash
# Build a debug binary first
cargo build -p aipm

# Then launch VS Code with the binary path set
AIPM_PATH=./target/debug/aipm code .
```

### Test fixture

`fixtures/extension-test/` contains a curated set of files that trigger all 17
lint rules. Run `aipm lint fixtures/extension-test` in the terminal to see all
expected diagnostics before launching the Extension Development Host.

See `fixtures/extension-test/WHAT_SHOULD_FAIL.md` for the full list of expected
failures with rule IDs and file paths.

---

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| No diagnostics in Problems panel | `aipm` binary not found | Set `aipm.path` in settings |
| No diagnostics in Problems panel | `aipm.lint.enable` is `false` | Enable in settings |
| No completions in `aipm.toml` | Even Better TOML not installed | Install `tamasfe.even-better-toml` |
| Stale diagnostics after file close | Extension is running correctly; diagnostics are cleared on close | No action needed |
| Extension not activating | No `aipm.toml` in workspace | Create `aipm.toml` or open a folder that contains one |

---

## See also

- [`aipm lint` reference](./lint.md) — CLI flags, output formats, and CI integration
- [Configuring lint rules](./configuring-lint.md) — severity overrides, path ignores
- [Lint rule reference](../rules/) — individual rule pages
- [JSON Schema for `aipm.toml`](../../schemas/aipm.toml.schema.json) — bundled schema source
