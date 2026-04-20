---
date: 2026-04-19 14:12:00 UTC
researcher: Claude Opus 4.6
git_commit: 3a011b63585b16c61f076ae812704c01a383f5de
branch: main
repository: aipm
topic: "Full editor experience for aipm.toml — syntax highlighting, schema validation, autocomplete, formatting, and file icons"
tags: [research, codebase, toml, schema, vscode, lsp, taplo, tombi, schemastore, formatting, icons]
status: complete
last_updated: 2026-04-19
last_updated_by: Claude Opus 4.6
---

# Research: Full Editor Experience for `aipm.toml`

## Research Question

How do I get the `aipm.toml` file to look beautiful and formatted in VS Code and other editors? What requirements does that need? Scope: syntax highlighting, JSON Schema validation/autocomplete, formatting, file icons, and LSP integration — the complete story.

## Summary

The aipm project already has substantial infrastructure for editor support: a JSON Schema (`schemas/aipm.toml.schema.json`), a VS Code extension (`vscode-aipm/`), and a custom LSP server (`crates/aipm/src/lsp.rs`) providing diagnostics, completions, and hover documentation. However, several gaps remain before the experience matches what Cargo.toml users enjoy. This document catalogs what exists today, what the external TOML editor ecosystem provides, and what specific pieces are needed to close the gaps.

## Detailed Findings

### 1. Current State — What Exists Today

#### 1.1 JSON Schema (Partial — `[workspace.lints]` Only)

**File:** [`schemas/aipm.toml.schema.json`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/schemas/aipm.toml.schema.json)

- Draft: JSON Schema **Draft-04** (`http://json-schema.org/draft-04/schema#`)
- `$id`: `https://raw.githubusercontent.com/TheLarkInn/aipm/main/schemas/aipm.toml.schema.json`
- Covers: `[workspace.lints]` section only — rule ID enum, severity values (`allow`/`warn`/`warning`/`error`/`deny`), global ignore paths, per-rule `{ level, ignore }` objects, and `instructions/oversized` custom options (`lines`, `characters`, `resolve-imports`)
- Uses `x-taplo.initKeys` for `["workspace.lints"]`
- Top-level `additionalProperties: true` — all other sections pass through unconstrained
- **Does NOT cover**: `[package]`, `[workspace]`, `[dependencies]`, `[components]`, `[environment]`, `[install]`, `[features]`, `[overrides]`, `[catalog]`, `[catalogs]`

A bundled copy lives at [`vscode-aipm/schemas/aipm.toml.schema.json`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/vscode-aipm/schemas/aipm.toml.schema.json).

#### 1.2 VS Code Extension

**Directory:** [`vscode-aipm/`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/vscode-aipm/package.json)

| Aspect | Current State |
|--------|---------------|
| Activation | `workspaceContains:**/aipm.toml` |
| Language client | Launches `aipm lsp` via stdio |
| Schema registration | `tomlValidation` contribution point for `aipm.toml` |
| Document selector | 16 file patterns (aipm.toml, skills, agents, hooks, plugins, instruction files) |
| Settings | `aipm.lint.enable` (bool), `aipm.path` (string) |
| `extensionDependencies` | **None** — Even Better TOML must be installed separately |
| Marketplace | **Not published** — install from source only |
| File icon | **None** |

**Source:** [`vscode-aipm/src/extension.ts`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/vscode-aipm/src/extension.ts)

#### 1.3 LSP Server

**Files:** [`crates/aipm/src/lsp.rs`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/crates/aipm/src/lsp.rs), [`crates/aipm/src/lsp/helpers.rs`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/crates/aipm/src/lsp/helpers.rs)

| Capability | Implementation |
|------------|---------------|
| `textDocument/publishDiagnostics` | Runs `aipm lint` on open and save (300ms debounce) |
| `textDocument/completion` | Rule ID completions + severity value completions in `[workspace.lints]` |
| `textDocument/hover` | Rule name, default severity, help text, documentation link |
| Transport | stdio (`tower-lsp` crate) |
| Sync mode | `TextDocumentSyncKind::NONE` — reads from disk, not editor buffer |

The LSP helpers module builds a rule index from `libaipm::lint::rules::catalog()` (18 rules) and converts aipm diagnostics to LSP diagnostics with 1-to-0-based line/col conversion.

#### 1.4 SchemaStore Submission (Prepared, Not Submitted)

**File:** [`schemas/schemastore-submission/catalog-entry.json`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/schemas/schemastore-submission/catalog-entry.json)

```json
{
  "name": "aipm.toml",
  "description": "AI Package Manager configuration",
  "fileMatch": ["aipm.toml"],
  "url": "https://json.schemastore.org/aipm.toml.json"
}
```

Test files exist at:
- `schemas/schemastore-submission/test/valid.toml`
- `schemas/schemastore-submission/test/invalid.toml`
- `schemas/tests/` (5 test files: valid, valid-package-only, valid-with-dependencies, invalid-unknown-rule, invalid-wrong-value-type)

#### 1.5 Manifest Struct (Rust — The Canonical Schema Source)

**File:** [`crates/libaipm/src/manifest/types.rs`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/crates/libaipm/src/manifest/types.rs)

The `Manifest` struct uses `#[serde(deny_unknown_fields)]` and defines:

| Section | Type | Key Fields |
|---------|------|------------|
| `[package]` | `Package` | `name`, `version`, `description`, `type`, `files`, `engines`, `source` |
| `[workspace]` | `Workspace` | `members`, `plugins_dir`, `dependencies` |
| `[dependencies]` | `BTreeMap<String, DependencySpec>` | Simple string or detailed (`version`, `workspace`, `git`, `github`, `path`, `marketplace`, `features`, etc.) |
| `[overrides]` | `BTreeMap<String, String>` | Version override strings |
| `[components]` | `Components` | `skills`, `commands`, `agents`, `hooks`, `mcp_servers`, `lsp_servers`, `scripts`, `output_styles`, `settings` |
| `[features]` | `BTreeMap<String, Vec<String>>` | Feature name → dependency list |
| `[environment]` | `Environment` | `requires`, `aipm`, `platforms`, `strict`, `variables`, `runtime` |
| `[install]` | `Install` | `allowed_build_scripts` |
| `[catalog]` | `BTreeMap<String, String>` | Default catalog |
| `[catalogs]` | `BTreeMap<String, BTreeMap<String, String>>` | Named catalogs |

The `[workspace.lints]` section is parsed separately in [`crates/aipm/src/main.rs:744-832`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/crates/aipm/src/main.rs#L744-L832) via raw `toml::Value` navigation, not through the `Manifest` struct.

---

### 2. External Ecosystem — How TOML Gets Beautiful in Editors

#### 2.1 The Two TOML Language Servers

| | **Taplo (Even Better TOML)** | **Tombi** |
|---|---|---|
| GitHub | [tamasfe/taplo](https://github.com/tamasfe/taplo) (2.2k stars) | [tombi-toml/tombi](https://github.com/tombi-toml/tombi) (864 stars) |
| VS Code installs | **4.2M+** | ~11K |
| JSON Schema drafts | Draft-04 only | Draft-07, 2019-09, 2020-12 |
| SchemaStore integration | Yes (auto-fetches catalog) | Yes (auto-fetches catalog) |
| Schema-aware key sorting | No | Yes (`x-tombi-table-keys-order`) |
| Safe formatting | Known bug: deletes data on incomplete TOML | Never deletes user data |
| Custom extensions | `x-taplo` (initKeys, docs, links, hidden, plugins) | `x-tombi-*` (toml-version, table-keys-order, array-values-order, string-formats) |
| Config file | `.taplo.toml` | `tombi.toml` |
| Editor support | VS Code, Neovim, Helix, Emacs (via LSP) | VS Code, Zed, JetBrains, Helix, Emacs (via LSP) |

**Both read SchemaStore's catalog automatically**, meaning a SchemaStore submission gives zero-config schema support to all users of either tool.

#### 2.2 Schema Association Methods (5 Ways)

| Method | Who Benefits | Effort |
|--------|-------------|--------|
| **SchemaStore submission** | All taplo + tombi users (millions) | One PR to SchemaStore repo |
| **VS Code `tomlValidation` contribution** | Users who install the `vscode-aipm` extension | Already implemented |
| **`.taplo.toml` in project** | Contributors to repos with aipm.toml | Add file to repo |
| **`#:schema` directive in TOML** | Users who add the directive to their aipm.toml | Per-file, manual |
| **`evenBetterToml.schema.associations` setting** | Individual VS Code users | Per-user, manual |

#### 2.3 How Cargo.toml Does It (Reference Model)

Cargo.toml achieves its editor experience through:

1. **SchemaStore catalog entry**: `fileMatch: ["Cargo.toml"]` → `https://json.schemastore.org/cargo.json`
2. **Even Better TOML / Taplo**: Auto-fetches SchemaStore catalog, matches `Cargo.toml`, applies schema
3. **Rich `x-taplo` extensions**: `links.key` for doc links, `docs.enumValues` for enum descriptions, `hidden` for deprecated fields, `initKeys` for table creation, `plugins: ["crates"]` for crates.io lookups
4. **`x-tombi-*` extensions**: `table-keys-order: "schema"`, `toml-version: "v1.0.0"`, `additional-key-label` for dynamic keys
5. **Schema is Draft-07** (the SchemaStore-recommended version)

rust-analyzer does NOT provide Cargo.toml editing support — it's all via taplo + SchemaStore.

#### 2.4 SchemaStore Submission Process

Required files for a PR to [SchemaStore/schemastore](https://github.com/SchemaStore/schemastore):

| File | Purpose |
|------|---------|
| `src/schemas/json/aipm.json` | The JSON Schema itself |
| `src/test/aipm/aipm.toml` | Positive test file (must validate) |
| `src/negative_test/aipm/aipm.toml` | Negative test file (must fail) |
| Entry in `src/api/json/catalog.json` | Catalog entry (alphabetical order) |

SchemaStore recommends **Draft-07** (`http://json-schema.org/draft-07/schema#`). CLI helper: `node cli.js new-schema` (interactive), `node cli.js check --schema-name=aipm.json` (validate).

Key links:
- [CONTRIBUTING.md](https://github.com/SchemaStore/schemastore/blob/master/CONTRIBUTING.md)
- [catalog.json](https://github.com/SchemaStore/schemastore/blob/master/src/api/json/catalog.json)

#### 2.5 Taplo Formatting Configuration

A `.taplo.toml` file controls TOML formatting. Key options:

```toml
include = ["**/*.toml"]
exclude = ["target/**"]

[formatting]
align_entries = false        # Vertical alignment of key = value
align_comments = true        # Align trailing comments
array_trailing_comma = true  # Trailing comma in multiline arrays
array_auto_expand = true     # Expand arrays past column_width
array_auto_collapse = true   # Collapse short arrays to one line
compact_arrays = true        # No padding in single-line arrays
compact_inline_tables = false
column_width = 80
indent_string = "  "
trailing_newline = true
reorder_keys = false         # Alphabetical key sorting
allowed_blank_lines = 2
crlf = false

# Schema association (alternative to SchemaStore)
[[rule]]
include = ["**/aipm.toml"]
[rule.schema]
path = "https://raw.githubusercontent.com/TheLarkInn/aipm/main/schemas/aipm.toml.schema.json"
```

Docs: [taplo formatter options](https://taplo.tamasfe.dev/configuration/formatter-options.html), [configuration file](https://taplo.tamasfe.dev/configuration/file.html)

#### 2.6 VS Code File Icons

Three approaches for a custom `aipm.toml` icon:

| Approach | Reach | Effort |
|----------|-------|--------|
| **Material Icon Theme `customClones`** setting | Individual users | Zero — add to `settings.json` |
| **Upstream PR to Material Icon Theme** | 20M+ users | SVG icon + PR to [material-extensions/vscode-material-icon-theme](https://github.com/material-extensions/vscode-material-icon-theme) |
| **`contributes.iconThemes`** in `vscode-aipm` | Extension users | SVG icon + icon theme JSON |

User-level custom clone example:
```json
"material-icon-theme.files.customClones": [
  {
    "name": "aipm",
    "base": "json",
    "color": "#7C3AED",
    "fileNames": ["aipm.toml"]
  }
]
```

Upstream acceptance bar: demonstrated community adoption (GitHub stars, real-world usage).

#### 2.7 JSON Schema Draft Versions

| Draft | SchemaStore | Taplo | Tombi | Recommendation |
|-------|-------------|-------|-------|----------------|
| Draft-04 | Supported | Full | Undocumented | Current aipm schema uses this |
| **Draft-07** | **Recommended** | Supported | Full | **Use this** |
| 2019-09 | Not recommended | Not supported | Full | Avoid |
| 2020-12 | Not recommended | Not supported | Full | Avoid |

TOML-specific limitations:
- `nan`/`inf`/`-inf` are valid TOML floats but invalid JSON numbers — use dual-type `["number", "string"]` if needed
- TOML has no `null` — use `required` property lists instead of nullable types
- Datetime types map to `string` with `format: "date-time"` / `format: "date"` / `format: "time"`

---

### 3. Gap Analysis — What's Missing

| Area | Current State | Gap | Priority |
|------|---------------|-----|----------|
| **JSON Schema coverage** | `[workspace.lints]` only | Missing: `[package]`, `[workspace]`, `[dependencies]`, `[components]`, `[environment]`, `[install]`, `[features]`, `[overrides]`, `[catalog]`, `[catalogs]` | **High** |
| **Schema draft version** | Draft-04 | Should be **Draft-07** (SchemaStore recommended, both tools support) | **High** |
| **`x-taplo` extensions** | `initKeys` only | Missing: `links.key` (doc links per field), `docs.enumValues` (enum descriptions), `hidden` (deprecated fields) | Medium |
| **`x-tombi-*` extensions** | None | Missing: `table-keys-order`, `toml-version`, `array-values-order` | Medium |
| **SchemaStore submission** | Catalog entry prepared, not submitted | Submit PR to SchemaStore repo | **High** |
| **`.taplo.toml` for formatting** | None | Ship default formatting config for aipm projects | Medium |
| **`tombi.toml`** | None | Optional — for tombi-specific features | Low |
| **VS Code extension publishing** | Install from source only | Publish to VS Code Marketplace | **High** |
| **Even Better TOML dependency** | Not declared | Add `extensionPack` or `extensionDependencies` recommendation | Medium |
| **Custom file icon** | None | Material Icon Theme custom clone → eventual upstream PR | Low |
| **Syntax highlighting** | Works via Even Better TOML (standard TOML grammar) | No gap — standard TOML highlighting is sufficient | None |
| **LSP completions** | `[workspace.lints]` rule IDs + severities | Could extend to other sections (dependency names, component paths) | Low (future) |

---

### 4. Requirements for the Full Experience

#### Tier 1: Zero-Config Experience (Highest Impact)

**Expand the JSON Schema to cover the full manifest.**

The schema at `schemas/aipm.toml.schema.json` needs to describe every section from the `Manifest` struct in `types.rs`. This gives users autocomplete, hover documentation, and validation for all fields — not just lint configuration.

Key sections to add:
- `[package]`: `name` (string, pattern `^(@[a-z0-9-]+/)?[a-z0-9][a-z0-9-]*$`), `version` (string, semver), `description`, `type` (enum: skill/agent/mcp/hook/lsp/composite), `files`, `engines` (enum items: claude/copilot), `source` (object: type, url, path)
- `[workspace]`: `members` (array of glob strings), `plugins_dir` (string), `dependencies`
- `[dependencies]`: Map of string (simple) or object (detailed: version, workspace, git, github, path, marketplace, features, etc.)
- `[components]`: All 9 component arrays (skills, commands, agents, hooks, mcp_servers, lsp_servers, scripts, output_styles, settings)
- `[environment]`: requires, aipm, platforms, strict, variables, runtime
- `[install]`: allowed_build_scripts
- `[features]`: Map of string arrays
- `[overrides]`: Map of strings
- `[catalog]` / `[catalogs]`: Map of strings / Map of maps

**Upgrade the schema to Draft-07** and add `x-taplo` + `x-tombi-*` extensions.

**Submit to SchemaStore.** This single action gives zero-config support to every taplo/tombi user worldwide.

#### Tier 2: Formatting and Project Configuration

**Ship a `.taplo.toml`** in the aipm repo with recommended formatting settings:

```toml
[[rule]]
include = ["**/aipm.toml"]
[rule.schema]
path = "https://raw.githubusercontent.com/TheLarkInn/aipm/main/schemas/aipm.toml.schema.json"

[formatting]
column_width = 100
trailing_newline = true
reorder_keys = false
array_trailing_comma = true
```

This ensures consistent formatting for aipm.toml files across all editors.

#### Tier 3: VS Code Polish

- **Publish `vscode-aipm` to the Marketplace** — currently install-from-source only
- **Add `extensionDependencies`** on Even Better TOML (or as `extensionPack` recommendation) for schema validation to work out of the box
- **Add a custom file icon** via `contributes.iconThemes` in `package.json` — requires an SVG icon

#### Tier 4: Future Enhancements

- **Extend LSP completions** beyond `[workspace.lints]` — dependency names, component paths, feature names
- **Schema auto-generation** from Rust types using `schemars` crate with `#[derive(JsonSchema)]` — keeps schema in sync with code
- **Submit icon to Material Icon Theme** upstream after community adoption grows
- **Text document sync** — switch LSP from disk-based reads to buffer content for real-time validation before save

---

## Code References

- [`schemas/aipm.toml.schema.json`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/schemas/aipm.toml.schema.json) — Current JSON Schema (lints only)
- [`vscode-aipm/schemas/aipm.toml.schema.json`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/vscode-aipm/schemas/aipm.toml.schema.json) — Bundled copy in VS Code extension
- [`vscode-aipm/package.json`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/vscode-aipm/package.json) — Extension manifest with `tomlValidation` contribution
- [`vscode-aipm/src/extension.ts`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/vscode-aipm/src/extension.ts) — Extension entry point (language client setup)
- [`crates/aipm/src/lsp.rs`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/crates/aipm/src/lsp.rs) — LSP server (diagnostics, completions, hover)
- [`crates/aipm/src/lsp/helpers.rs`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/crates/aipm/src/lsp/helpers.rs) — Sync helper functions (rule index, completion context, diagnostic conversion)
- [`crates/libaipm/src/manifest/types.rs`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/crates/libaipm/src/manifest/types.rs) — Manifest struct definitions (canonical schema source)
- [`crates/libaipm/src/manifest/validate.rs`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/crates/libaipm/src/manifest/validate.rs) — Manifest validation logic
- [`schemas/schemastore-submission/catalog-entry.json`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/schemas/schemastore-submission/catalog-entry.json) — Prepared SchemaStore catalog entry
- [`docs/guides/vscode-extension.md`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/docs/guides/vscode-extension.md) — VS Code extension user guide
- [`docs/guides/configuring-lint.md`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/docs/guides/configuring-lint.md) — Lint configuration guide (references schema support)

## Architecture Documentation

### How the Editor Experience Stack Works

```
User opens aipm.toml in VS Code
        │
        ├─→ Even Better TOML (taplo) activates
        │   ├─→ Fetches SchemaStore catalog.json
        │   ├─→ Matches "aipm.toml" → schema URL
        │   ├─→ Provides: syntax highlighting, validation, autocomplete, hover, formatting
        │   └─→ Reads .taplo.toml for formatting config
        │
        └─→ vscode-aipm extension activates (workspaceContains:**/aipm.toml)
            ├─→ Launches `aipm lsp` via stdio
            ├─→ LSP provides: aipm lint diagnostics, rule ID completions, hover docs
            └─→ tomlValidation contribution → Even Better TOML uses bundled schema
```

These two systems are complementary:
- **Even Better TOML + schema**: Provides TOML-level intelligence (syntax, key/value types, enums, descriptions)
- **aipm LSP**: Provides domain-level intelligence (lint rule violations across all project files, not just aipm.toml)

### Schema Resolution Priority

When multiple schema sources exist, taplo resolves in this order (highest first):
1. Manual environment settings (CLI flags, IDE settings)
2. `#:schema` directives at the document top
3. `$schema` key in the document root
4. `.taplo.toml` configuration file rules
5. Default configuration schema
6. VS Code extension `tomlValidation` contributions
7. **SchemaStore catalog associations**

The `vscode-aipm` extension's `tomlValidation` (priority 6) takes precedence over SchemaStore (priority 7), ensuring extension users get the bundled schema even before a SchemaStore submission lands.

## Historical Context (from research/)

- [`research/docs/2026-04-10-377-vscode-support-aipm-lint.md`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/research/docs/2026-04-10-377-vscode-support-aipm-lint.md) — Original research for VS Code support (Issue #377). At the time, no JSON Schema, VS Code extension, or LSP existed. All three have since been implemented.
- [`research/docs/2026-03-09-manifest-format-comparison.md`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/research/docs/2026-03-09-manifest-format-comparison.md) — Rationale for choosing TOML over JSON/JSONC/YAML for the manifest format.
- [`research/docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/research/docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md) — How `aipm.toml` files are generated during `init` and `migrate`.
- [`research/tickets/2026-04-11-426-dogfood-aipm-lint.md`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/research/tickets/2026-04-11-426-dogfood-aipm-lint.md) — Dogfooding aipm lint in this repo; references taplo and Even Better TOML.
- [`specs/2026-04-10-vscode-aipm-lint-integration.md`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/specs/2026-04-10-vscode-aipm-lint-integration.md) — Technical design for VS Code + LSP integration.

## Related Research

- [`research/docs/2026-03-26-edition-field-purpose-and-rationale.md`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/research/docs/2026-03-26-edition-field-purpose-and-rationale.md) — Edition field design (relevant for schema enum values)
- [`research/docs/2026-04-02-aipm-lint-configuration-research.md`](https://github.com/TheLarkInn/aipm/blob/3a011b63585b16c61f076ae812704c01a383f5de/research/docs/2026-04-02-aipm-lint-configuration-research.md) — Lint config system design (ignore paths, per-rule overrides)

## External References

### TOML Editor Tools
- [Even Better TOML (VS Code)](https://marketplace.visualstudio.com/items?itemName=tamasfe.even-better-toml) — 4.2M installs
- [Tombi (VS Code)](https://marketplace.visualstudio.com/items?itemName=tombi-toml.tombi) — 11K installs
- [Taplo GitHub](https://github.com/tamasfe/taplo)
- [Tombi GitHub](https://github.com/tombi-toml/tombi)
- [Taplo Configuration File](https://taplo.tamasfe.dev/configuration/file.html)
- [Taplo Formatter Options](https://taplo.tamasfe.dev/configuration/formatter-options.html)
- [Taplo Developing Schemas](https://taplo.tamasfe.dev/configuration/developing-schemas.html)
- [Taplo Using Schemas](https://taplo.tamasfe.dev/configuration/using-schemas.html)
- [Tombi Configuration](https://tombi-toml.github.io/tombi/docs/configuration/)
- [Tombi JSON Schema](https://tombi-toml.github.io/tombi/docs/json-schema/)
- [Tombi vs Taplo Differences](https://tombi-toml.github.io/tombi/docs/reference/difference-taplo/)

### SchemaStore
- [SchemaStore.org](https://www.schemastore.org/)
- [SchemaStore GitHub](https://github.com/SchemaStore/schemastore)
- [CONTRIBUTING.md](https://github.com/SchemaStore/schemastore/blob/master/CONTRIBUTING.md)
- [Catalog JSON](https://github.com/SchemaStore/schemastore/blob/master/src/api/json/catalog.json)
- [SchemaStore Cargo.toml Schema](https://github.com/SchemaStore/schemastore/blob/master/src/schemas/json/cargo.json) — Reference for `x-taplo` extension usage

### JSON Schema and TOML
- [JSON Schema Everywhere — TOML](https://json-schema-everywhere.github.io/toml)
- [TOML Schema Discussion (toml-lang)](https://github.com/toml-lang/toml/discussions/1038)

### VS Code
- [VS Code File Icon Theme Guide](https://code.visualstudio.com/api/extension-guides/file-icon-theme)
- [Material Icon Theme](https://github.com/material-extensions/vscode-material-icon-theme)
- [VS Code Language Server Extension Guide](https://code.visualstudio.com/api/language-extensions/language-server-extension-guide)

### Reference Projects
- [Cargo Schema Tracking Issue (rust-lang/cargo#12883)](https://github.com/rust-lang/cargo/issues/12883)
- [rust-analyzer Cargo.toml Feature Tracking (rust-analyzer#15741)](https://github.com/rust-lang/rust-analyzer/issues/15741)
- [Ruff SchemaStore PR #2724](https://github.com/SchemaStore/schemastore/pull/2724) — Example of submitting a tool's schema

## Open Questions

1. **Schema generation strategy**: Should the full JSON Schema be hand-authored (like the current lints-only schema) or auto-generated from the Rust `Manifest` struct using `schemars` + `#[derive(JsonSchema)]`? Auto-generation keeps the schema in sync but may need manual `x-taplo` and `x-tombi-*` annotations layered on top.

2. **Schema hosting**: The current `$id` points to `raw.githubusercontent.com`. After SchemaStore submission, the canonical URL becomes `https://json.schemastore.org/aipm.json`. Should the aipm repo's schema be the source of truth that SchemaStore references (like Ruff), or should SchemaStore host a copy (like Cargo)?

3. **`[workspace.lints]` in full schema**: The lint config is parsed separately from the `Manifest` struct. The schema must describe both the serde-derived fields AND the raw-TOML lint config. Currently only lints are in the schema — the full schema needs to merge both.

4. **Extension marketplace publishing**: What publisher account will be used? What's the minimum bar for a v0.1.0 marketplace release?

5. **Icon design**: What should the aipm.toml file icon look like? Does it need to align with an existing aipm brand/logo?
