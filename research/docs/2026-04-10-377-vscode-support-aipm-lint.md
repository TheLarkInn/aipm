---
date: 2026-04-10 15:17:32 UTC
researcher: Claude Sonnet 4.6
git_commit: 42f0d05616c9bfee3f1555a2c583466eb5b9467e
branch: main
repository: aipm
topic: "VS Code Support for aipm lint (Issue #377)"
tags: [research, codebase, lint, vscode, lsp, schema, toml, syntax-highlighting, autocomplete]
status: complete
last_updated: 2026-04-10
last_updated_by: Claude Sonnet 4.6
---

# Research: VS Code Support for aipm lint

## Research Question

[Issue #377](https://github.com/TheLarkInn/aipm/issues/377) — What is needed to add VS Code support for `aipm lint`? The issue lists four items:

1. LSP
2. `aipm.toml` schema file (for linting only, initially)
3. `aipm.toml` syntax highlighting
4. `aipm.toml` syntax autocomplete

## Summary

The `aipm lint` command is a mature, 19-rule linting system with config-driven severity overrides, ignore paths, and four output reporters (human, JSON, GitHub Actions, Azure DevOps). The `aipm.toml` manifest format is fully defined in Rust structs with `#[serde(deny_unknown_fields)]` enforcement and semantic validation. **No JSON Schema, TOML schema, or VS Code extension code exists today.** The technical design spec at `specs/2026-03-09-aipm-technical-design.md:310` mentions a plan to "publish a JSON Schema via SchemaStore for IDE autocomplete via Taplo," but no implementation has been started.

The dominant approach for VS Code TOML support is to leverage the **taplo** ecosystem (Even Better TOML extension) with a JSON Schema. This provides validation, autocomplete, and hover documentation with zero custom LSP code. A custom `aipm lsp` subcommand would only be needed later for domain-specific features (dependency resolution, workspace navigation).

## Detailed Findings

### 1. Current `aipm lint` Implementation

#### CLI Entry Point

The lint command is defined at [`crates/aipm/src/main.rs:157-181`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/aipm/src/main.rs#L157-L181) as a clap `Commands::Lint` variant with these arguments:

| Argument | Type | Default | Description |
|---|---|---|---|
| `dir` | `PathBuf` | `"."` | Project directory to lint |
| `--source` | `Option<String>` | None | Filter to `.claude`, `.github`, or `.ai` |
| `--reporter` | `String` | `"human"` | Output format: `human`, `json`, `ci-github`, `ci-azure` |
| `--color` | `String` | `"auto"` | Color mode: `never`, `auto`, `always` |
| `--max-depth` | `Option<usize>` | None | Directory traversal depth limit |

The handler at [`crates/aipm/src/main.rs:662-741`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/aipm/src/main.rs#L662-L741) loads config from `aipm.toml`, calls `libaipm::lint::lint()`, and dispatches to a reporter.

#### Library Entry Point

[`crates/libaipm/src/lint/mod.rs:115-162`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/mod.rs#L115-L162) — The `lint()` function:

1. Calls `discover_features()` for a single gitignore-aware recursive walk
2. Optionally filters by `--source`
3. For each feature, calls `run_rules_for_feature()` which dispatches kind-specific rules
4. Sorts diagnostics by path/line/column, counts errors/warnings, returns `Outcome`

#### Discovery Pipeline

[`crates/libaipm/src/discovery.rs:280-350`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/discovery.rs#L280-L350) — Uses `ignore::WalkBuilder` for gitignore-aware traversal. `classify_feature_kind()` at [line 233](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/discovery.rs#L233) maps files to `FeatureKind`: `Skill`, `Agent`, `Hook`, `Plugin`, `Marketplace`, `PluginJson`.

#### All 19 Lint Rules

| Rule ID | Default | File | Description |
|---------|---------|------|-------------|
| `skill/missing-name` | Warn | [`rules/skill_missing_name.rs:13`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rules/skill_missing_name.rs#L13) | SKILL.md lacks `name` frontmatter |
| `skill/missing-description` | Warn | [`rules/skill_missing_desc.rs:13`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rules/skill_missing_desc.rs#L13) | SKILL.md lacks `description` frontmatter |
| `skill/oversized` | Warn | [`rules/skill_oversized.rs:18`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rules/skill_oversized.rs#L18) | SKILL.md exceeds 15,000 chars |
| `skill/name-too-long` | Warn | [`rules/skill_name_too_long.rs:18`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rules/skill_name_too_long.rs#L18) | `name` exceeds 64 chars |
| `skill/name-invalid-chars` | Warn | [`rules/skill_name_invalid.rs:29`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rules/skill_name_invalid.rs#L29) | `name` fails `/^[a-zA-Z0-9][a-zA-Z0-9._\- ]*$/` |
| `skill/description-too-long` | Warn | [`rules/skill_desc_too_long.rs:18`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rules/skill_desc_too_long.rs#L18) | `description` exceeds 1,024 chars |
| `skill/invalid-shell` | Error | [`rules/skill_invalid_shell.rs:18`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rules/skill_invalid_shell.rs#L18) | `shell` not in `["bash", "powershell"]` |
| `plugin/broken-paths` | Error | [`rules/broken_paths.rs:20`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rules/broken_paths.rs#L20) | `${CLAUDE_SKILL_DIR}/` refs point to missing files |
| `agent/missing-tools` | Warn | [`rules/agent_missing_tools.rs:13`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rules/agent_missing_tools.rs#L13) | Agent `.md` lacks `tools` frontmatter |
| `hook/unknown-event` | Error | [`rules/hook_unknown_event.rs:16`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rules/hook_unknown_event.rs#L16) | Hook event not in Claude/Copilot known events |
| `hook/legacy-event-name` | Warn | [`rules/hook_legacy_event.rs:15`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rules/hook_legacy_event.rs#L15) | PascalCase Copilot event should be camelCase |
| `source/misplaced-features` | Warn | [`rules/misplaced_features.rs:21`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rules/misplaced_features.rs#L21) | Feature found outside `.ai/` |
| `marketplace/source-resolve` | Error | [`rules/marketplace_source_resolve.rs:13`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rules/marketplace_source_resolve.rs#L13) | `marketplace.json` source path doesn't exist |
| `marketplace/plugin-field-mismatch` | Error | [`rules/marketplace_field_mismatch.rs:13`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rules/marketplace_field_mismatch.rs#L13) | `marketplace.json` and `plugin.json` fields differ |
| `plugin/missing-registration` | Error | [`rules/plugin_missing_registration.rs:15`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rules/plugin_missing_registration.rs#L15) | Plugin dir exists but isn't in `marketplace.json` |
| `plugin/missing-manifest` | Error | [`rules/plugin_missing_manifest.rs:14`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rules/plugin_missing_manifest.rs#L14) | Plugin dir lacks `.claude-plugin/plugin.json` |
| `plugin/required-fields` | Error | [`rules/plugin_required_fields.rs:13`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rules/plugin_required_fields.rs#L13) | `plugin.json` missing `name`/`description`/`version`/`author` |

Rules are dispatched by `FeatureKind` via [`rules/mod.rs:38-64`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rules/mod.rs#L38-L64).

#### Rule Trait

All rules implement the `Rule` trait at [`lint/rule.rs:16-50`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rule.rs#L16-L50): `id()`, `name()`, `default_severity()`, `help_url()`, `help_text()`, `check()`, `check_file()`. All filesystem access goes through `&dyn Fs`.

#### Diagnostic Model

[`lint/diagnostic.rs:39-63`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/diagnostic.rs#L39-L63) — Each `Diagnostic` carries: `rule_id`, `severity` (Warning/Error), `message`, `file_path`, `line`/`col`/`end_line`/`end_col`, `source_type`, `help_text`, `help_url`.

#### Config System

[`lint/config.rs`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/config.rs) — Config loaded from `[workspace.lints]` in `aipm.toml`. Supports:
- Global ignore paths: `[workspace.lints.ignore].paths`
- Per-rule overrides: `RuleOverride::Allow` (suppress), `RuleOverride::Level(Severity)` (change severity), `RuleOverride::Detailed { level, ignore }` (severity + per-rule ignore paths)

Config parsing is in [`crates/aipm/src/main.rs:744-832`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/aipm/src/main.rs#L744-L832).

#### Reporters

[`lint/reporter.rs`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/reporter.rs) — Four implementations:
1. **Human** (line 97) — Rich terminal output via `annotate-snippets` crate with rustc-style code spans
2. **Json** (line 239) — Structured JSON with `diagnostics` array and `summary`
3. **CiGitHub** (line 309) — `::error`/`::warning` GitHub Actions annotations
4. **CiAzure** (line 337) — `##vso[task.logissue]` Azure DevOps annotations

### 2. `aipm.toml` Manifest Format

#### Struct Definitions

All types live in [`crates/libaipm/src/manifest/types.rs`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/manifest/types.rs).

**`Manifest` (top-level, line 10)** — `#[serde(deny_unknown_fields)]`:

| Section | Type | Description |
|---------|------|-------------|
| `[package]` | `Option<Package>` | Package metadata (member manifests) |
| `[workspace]` | `Option<Workspace>` | Workspace config (root manifests) |
| `[dependencies]` | `Option<BTreeMap<String, DependencySpec>>` | Direct dependencies |
| `[overrides]` | `Option<BTreeMap<String, String>>` | Dependency overrides (root only) |
| `[components]` | `Option<Components>` | Component file declarations |
| `[features]` | `Option<BTreeMap<String, Vec<String>>>` | Feature definitions |
| `[environment]` | `Option<Environment>` | Environment requirements |
| `[install]` | `Option<Install>` | Installation behavior |
| `[catalog]` | `Option<BTreeMap<String, String>>` | Default catalog (root only) |
| `[catalogs]` | `Option<BTreeMap<String, BTreeMap<String, String>>>` | Named catalogs (root only) |

**`Package` (line 45)** — `#[serde(deny_unknown_fields)]`:

| Field | TOML key | Type | Required |
|-------|----------|------|----------|
| `name` | `name` | `String` | Yes (validated: `^(@[a-z0-9-]+/)?[a-z0-9][a-z0-9-]*$`) |
| `version` | `version` | `String` | Yes (semver) |
| `description` | `description` | `Option<String>` | No |
| `plugin_type` | `type` | `Option<String>` | No (enum: skill/agent/mcp/hook/lsp/composite) |
| `files` | `files` | `Option<Vec<String>>` | No |
| `engines` | `engines` | `Option<Vec<String>>` | No (values: `claude`, `copilot`) |
| `source` | `source` | `Option<SourceRedirect>` | No |

**`Workspace` (line 89)**:

| Field | Type | Required |
|-------|------|----------|
| `members` | `Vec<String>` | Yes (glob patterns) |
| `plugins_dir` | `Option<String>` | No |
| `dependencies` | `Option<BTreeMap<String, DependencySpec>>` | No |

**`DependencySpec` (line 102)** — `#[serde(untagged)]`: either `Simple(String)` (bare version) or `Detailed(DetailedDependency)`.

**`DetailedDependency` (line 116)**: `version`, `workspace` (only `"*"`), `optional`, `default-features`, `features`, `git`, `github`, `path`, `marketplace`, `name`, `ref`.

**`Components` (line 158)**: `skills`, `commands`, `agents`, `hooks`, `mcp_servers`, `lsp_servers`, `scripts`, `output_styles`, `settings` (all `Option<Vec<String>>`).

**`Environment` (line 189)**: `requires`, `aipm`, `platforms`, `strict`, `variables`, `runtime`.

**`Install` (line 243)**: `allowed_build_scripts`.

**`PluginType` (line 250)**: `skill`, `agent`, `mcp`, `hook`, `lsp`, `composite`.

#### Parsing & Validation

- **Parsing**: [`manifest/mod.rs:21`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/manifest/mod.rs#L21) — `toml::from_str::<Manifest>()` with `deny_unknown_fields`
- **Validation**: [`manifest/validate.rs:76`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/manifest/validate.rs#L76) — name format, semver version, plugin type enum, dependency versions, workspace protocol (`"*"` only), component path existence
- **Editing**: [`installer/manifest_editor.rs`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/installer/manifest_editor.rs) — uses `toml_edit` crate for comment-preserving round-trip editing

#### No Schema Exists Today

No JSON Schema, TOML schema, schema generation code, `.taplo.toml` config, or `SchemaStore` submission exists. The design spec mentions the plan at [`specs/2026-03-09-aipm-technical-design.md:310`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/specs/2026-03-09-aipm-technical-design.md#L310).

#### Example Fixtures

- [`fixtures/standalone-plugin/aipm.toml`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/fixtures/standalone-plugin/aipm.toml) — minimal `[package]` with name, version, type, description
- [`fixtures/workspace-no-deps/aipm.toml`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/fixtures/workspace-no-deps/aipm.toml) — `[workspace]` with members and plugins_dir
- [`fixtures/workspace-separate-plugins-dir/aipm.toml`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/fixtures/workspace-separate-plugins-dir/aipm.toml) — workspace with `[dependencies]` using workspace protocol
- [`fixtures/workspace-transitive-deps/aipm.toml`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/fixtures/workspace-transitive-deps/aipm.toml) — workspace with transitive dependency chain

### 3. VS Code Extension Ecosystem for TOML

#### Taplo / Even Better TOML

[Taplo](https://github.com/tamasfe/taplo) is the established TOML toolkit in Rust. Its VS Code extension [Even Better TOML](https://marketplace.visualstudio.com/items?itemName=tamasfe.even-better-toml) provides:
- Full TOML 1.0.0 LSP (diagnostics, completion, hover, formatting)
- Schema-driven validation against JSON Schema (Draft 4)
- Schema-driven autocomplete and hover documentation
- `tomlValidation` contribution point for third-party extensions to register schemas
- `x-taplo` extension fields for richer completion hints (`initKeys`, per-enum documentation)
- `#:schema` directive and `$schema` key for per-file schema association

Docs: [Using Schemas](https://taplo.tamasfe.dev/configuration/using-schemas.html), [Developing Schemas](https://taplo.tamasfe.dev/configuration/developing-schemas.html), [Configuration File](https://taplo.tamasfe.dev/configuration/file.html)

#### Tombi (Newer Alternative)

[Tombi](https://github.com/tombi-toml/tombi) is a newer TOML LSP addressing taplo limitations: better schema-based sorting, "Go to JSON Schema Definition" navigation, workspace-aware features, safer formatting. Docs: [Differences from Taplo](https://tombi-toml.github.io/tombi/docs/reference/difference-taplo/)

#### Schema Association Methods

There are five ways to associate a JSON Schema with `aipm.toml`:

1. **`#:schema` directive** in the TOML file itself:
   ```toml
   #:schema https://raw.githubusercontent.com/TheLarkInn/aipm/main/schemas/aipm.toml.schema.json
   ```

2. **`$schema` root key**:
   ```toml
   "$schema" = "https://example.com/aipm.toml.schema.json"
   ```

3. **`.taplo.toml` project config**:
   ```toml
   [[rule]]
   include = ["**/aipm.toml"]
   [rule.schema]
   path = "https://example.com/aipm.toml.schema.json"
   ```

4. **VS Code `evenBetterToml.schema.associations` setting**:
   ```json
   { "evenBetterToml.schema.associations": { ".*aipm\\.toml$": "https://..." } }
   ```

5. **`tomlValidation` contribution** from a VS Code extension (auto-associates for all users who install the extension)

#### SchemaStore Distribution

[SchemaStore.org](https://www.schemastore.org/) ([GitHub](https://github.com/SchemaStore/schemastore)) is the standard for distributing schemas. Once submitted, taplo and tombi automatically pick up the schema for matching filenames. Required: schema JSON file, `catalog.json` entry with `fileMatch: ["aipm.toml"]`, positive/negative test files.

#### Syntax Highlighting

VS Code has **no built-in TOML highlighting** — it comes from extensions. Both Even Better TOML and Tombi provide TextMate grammars with scope `source.toml`. Since `aipm.toml` is valid TOML, no custom grammar is needed. Standard TOML scopes (table headers, keys, strings, numbers, booleans, comments) work out of the box.

#### Autocomplete

Both taplo and tombi provide **schema-driven autocomplete**: table name suggestions, key suggestions within tables, enum value suggestions, hover documentation from `description` fields, and `x-taplo.initKeys` for auto-generating important keys on table creation.

#### VS Code Extension Structure (Minimal)

A declarative-only extension for schema contribution:

```
vscode-aipm/
  package.json          # Extension manifest with tomlValidation contribution
  schemas/
    aipm.toml.schema.json
```

```json
{
  "name": "vscode-aipm",
  "extensionDependencies": ["tamasfe.even-better-toml"],
  "contributes": {
    "tomlValidation": [{
      "fileMatch": "aipm.toml",
      "url": "./schemas/aipm.toml.schema.json"
    }]
  }
}
```

No TypeScript code needed. Source: [taplo issue #617](https://github.com/tamasfe/taplo/issues/617)

#### Reference Project: Pipelex

[Pipelex](https://github.com/Pipelex/vscode-pipelex) forked taplo to support `.mthds` files (custom TOML-based AI pipeline format). This is the Tier 3 "full fork" approach — registered custom languages, built a full LSP, added domain-specific semantic tokens. This level of engineering is not needed for `aipm.toml`.

### 4. Lint Config in `aipm.toml` (The Schema Intersection)

The lint configuration lives under `[workspace.lints]` in `aipm.toml`. This section is **not part of the `Manifest` struct** — it's parsed separately in the CLI handler at [`main.rs:744-832`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/aipm/src/main.rs#L744-L832) using raw `toml::Value` navigation.

Structure:

```toml
[workspace.lints]
# Per-rule overrides
"skill/missing-name" = "allow"              # suppress the rule
"skill/oversized" = "error"                 # change severity to error
"hook/unknown-event" = { level = "warn", ignore = ["legacy/**"] }  # detailed override

[workspace.lints.ignore]
paths = ["vendor/**", "third_party/**"]     # global ignore paths
```

This means the JSON Schema for `aipm.toml` needs to describe not just the `Manifest` struct fields but also the `[workspace.lints]` configuration that's parsed separately.

## Code References

### Lint Pipeline
- [`crates/aipm/src/main.rs:157-181`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/aipm/src/main.rs#L157-L181) — CLI command definition
- [`crates/aipm/src/main.rs:662-741`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/aipm/src/main.rs#L662-L741) — CLI handler
- [`crates/aipm/src/main.rs:744-832`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/aipm/src/main.rs#L744-L832) — Lint config loading from `aipm.toml`
- [`crates/libaipm/src/lint/mod.rs:115-162`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/mod.rs#L115-L162) — `lint()` library entry point
- [`crates/libaipm/src/lint/mod.rs:68-104`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/mod.rs#L68-L104) — `run_rules_for_feature()` dispatch
- [`crates/libaipm/src/lint/rules/mod.rs:38-64`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rules/mod.rs#L38-L64) — `quality_rules_for_kind()` factory
- [`crates/libaipm/src/lint/rule.rs:16-50`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rule.rs#L16-L50) — `Rule` trait
- [`crates/libaipm/src/lint/diagnostic.rs:39-63`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/diagnostic.rs#L39-L63) — `Diagnostic` struct
- [`crates/libaipm/src/lint/config.rs`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/config.rs) — `Config` and `RuleOverride`
- [`crates/libaipm/src/lint/reporter.rs`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/reporter.rs) — All four reporters

### Discovery
- [`crates/libaipm/src/discovery.rs:233-278`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/discovery.rs#L233-L278) — `classify_feature_kind()`
- [`crates/libaipm/src/discovery.rs:280-350`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/discovery.rs#L280-L350) — `discover_features()`

### Manifest
- [`crates/libaipm/src/manifest/types.rs`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/manifest/types.rs) — All struct/enum definitions
- [`crates/libaipm/src/manifest/mod.rs:21`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/manifest/mod.rs#L21) — `parse()` entry point
- [`crates/libaipm/src/manifest/validate.rs:76`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/manifest/validate.rs#L76) — `validate()` entry point
- [`crates/libaipm/src/manifest/error.rs`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/manifest/error.rs) — Error types

### Hook Events
- [`crates/libaipm/src/lint/rules/known_events.rs`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/crates/libaipm/src/lint/rules/known_events.rs) — 27 Claude events, 10 Copilot events, 10 legacy mappings

### Design Spec
- [`specs/2026-03-09-aipm-technical-design.md:310`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/specs/2026-03-09-aipm-technical-design.md#L310) — Planned JSON Schema / SchemaStore mention

## Architecture Documentation

### Lint Data Flow

1. `aipm lint [dir]` -> `cmd_lint()` loads `[workspace.lints]` config from `aipm.toml`
2. `libaipm::lint::lint()` calls `discover_features()` for a single gitignore-aware recursive walk
3. Each discovered feature is classified by `FeatureKind` (Skill/Agent/Hook/Plugin/Marketplace/PluginJson)
4. `run_rules_for_feature()` dispatches kind-specific rules + `misplaced-features` for non-`.ai/` features
5. Each rule's `check_file()` reads files via `&dyn Fs` trait and returns `Vec<Diagnostic>`
6. `apply_rule_diagnostics()` applies config overrides (suppress, severity change, ignore paths)
7. Diagnostics are sorted, counted, and rendered by the selected reporter

### Key Design Patterns

- **Filesystem abstraction**: All file I/O uses `&dyn Fs` trait (`crates/libaipm/src/fs.rs`), enabling `MockFs` in tests and `Real` in production
- **Single-pass discovery**: One recursive walk classifies all features, avoiding per-kind scans
- **Trait-based rules**: All rules implement `Rule` trait for uniform dispatch
- **Config-driven behavior**: Rules can be suppressed, re-leveled, or have ignore paths — all from `aipm.toml`
- **`deny_unknown_fields`**: `Manifest` and `Package` structs reject unknown TOML keys at parse time

### What an LSP Would Need to Reuse

The lint library (`libaipm::lint`) already:
- Accepts a filesystem trait (could adapt to virtual document buffers)
- Returns structured diagnostics with file positions
- Has a JSON reporter (structured output format)
- Is fully decoupled from the CLI

An `aipm lsp` subcommand could wrap `libaipm::lint::lint()` to provide LSP `textDocument/publishDiagnostics`. The `Diagnostic` struct already has line/column positions compatible with LSP `Position`.

## Historical Context (from research/)

- [`research/docs/2026-03-31-110-aipm-lint-architecture-research.md`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/research/docs/2026-03-31-110-aipm-lint-architecture-research.md) — Initial lint architecture research (Issue #110), established the Rule trait + discovery pipeline design
- [`research/docs/2026-04-02-aipm-lint-configuration-research.md`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/research/docs/2026-04-02-aipm-lint-configuration-research.md) — Lint config system design (ignore paths, per-rule overrides)
- [`research/docs/2026-04-07-lint-rules-287-288-289-290.md`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/research/docs/2026-04-07-lint-rules-287-288-289-290.md) — Marketplace/plugin lint rules research
- [`research/docs/2026-03-09-manifest-format-comparison.md`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/research/docs/2026-03-09-manifest-format-comparison.md) — Why TOML was chosen over JSON/JSONC/YAML for the manifest format
- [`research/docs/2026-03-24-claude-code-mcp-lsp-config.md`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/research/docs/2026-03-24-claude-code-mcp-lsp-config.md) — Claude Code MCP and LSP server configuration (the only existing LSP-related research)
- [`research/tickets/2026-03-28-110-aipm-lint.md`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/research/tickets/2026-03-28-110-aipm-lint.md) — Issue #110 ticket research
- [`research/tickets/2026-04-03-198-lint-display-ux.md`](https://github.com/TheLarkInn/aipm/blob/42f0d05616c9bfee3f1555a2c583466eb5b9467e/research/tickets/2026-04-03-198-lint-display-ux.md) — Lint display UX research

## Related Research

- `research/docs/2026-03-09-manifest-format-comparison.md` — TOML format choice rationale
- `research/docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md` — How `aipm.toml` files are generated
- `research/docs/2026-03-26-edition-field-purpose-and-rationale.md` — Edition field design
- `research/docs/2026-04-06-plugin-system-feature-parity-analysis.md` — Feature parity analysis covering manifest types
- `research/docs/2026-03-10-microsoft-apm-analysis.md` — Competitive analysis (microsoft/apm)

## External References

- [Taplo Documentation](https://taplo.tamasfe.dev/)
- [Taplo — Using Schemas](https://taplo.tamasfe.dev/configuration/using-schemas.html)
- [Taplo — Developing Schemas](https://taplo.tamasfe.dev/configuration/developing-schemas.html)
- [Taplo — Configuration File](https://taplo.tamasfe.dev/configuration/file.html)
- [Taplo — Directives](https://taplo.tamasfe.dev/configuration/directives.html)
- [Even Better TOML (VS Code)](https://marketplace.visualstudio.com/items?itemName=tamasfe.even-better-toml)
- [Taplo GitHub](https://github.com/tamasfe/taplo)
- [Taplo Issue #617 — tomlValidation contribution point](https://github.com/tamasfe/taplo/issues/617)
- [Tombi GitHub](https://github.com/tombi-toml/tombi)
- [Tombi — Differences from Taplo](https://tombi-toml.github.io/tombi/docs/reference/difference-taplo/)
- [SchemaStore.org](https://www.schemastore.org/) / [GitHub](https://github.com/SchemaStore/schemastore)
- [SchemaStore CONTRIBUTING.md](https://github.com/SchemaStore/schemastore/blob/master/CONTRIBUTING.md)
- [JSON Schema Everywhere — TOML](https://json-schema-everywhere.github.io/toml)
- [TOML Schema Discussion (toml-lang)](https://github.com/toml-lang/toml/discussions/1038)
- [Pipelex VS Code Extension (taplo fork)](https://github.com/Pipelex/vscode-pipelex)
- [VS Code — Language Server Extension Guide](https://code.visualstudio.com/api/language-extensions/language-server-extension-guide)
- [VS Code — Syntax Highlight Guide](https://code.visualstudio.com/api/language-extensions/syntax-highlight-guide)
- [Ruff VS Code Extension (Rust LSP reference)](https://github.com/astral-sh/ruff-vscode)

## Open Questions

1. **Schema generation**: Should the JSON Schema be hand-written or auto-generated from the Rust structs (e.g., via `schemars` crate with `#[derive(JsonSchema)]`)? Auto-generation keeps the schema in sync but may need manual `x-taplo` annotations.
2. **`[workspace.lints]` in schema**: The lint config section is parsed separately from the `Manifest` struct (via raw `toml::Value`). The schema needs to describe both the serde-derived structure AND the lint config that lives in `workspace.lints`.
3. **SchemaStore vs bundled**: Should the schema be submitted to SchemaStore first (zero-extension path) or bundled in a VS Code extension? These are not mutually exclusive.
4. **taplo vs tombi**: taplo is more established but has maintenance concerns. tombi is newer. The schema approach works with both.
5. **Custom LSP scope**: If an `aipm lsp` subcommand is built, what domain-specific features beyond schema validation would it provide? Dependency resolution hints? Workspace member navigation? Cross-file reference validation?
