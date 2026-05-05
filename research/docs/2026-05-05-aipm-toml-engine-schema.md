---
date: 2026-05-05
researcher: Sean Larkin
git_commit: ad39977
branch: main
repository: aipm
topic: "aipm.toml engine schema"
tags: [research, codebase, manifest, aipm-toml]
status: complete
last_updated: 2026-05-05
last_updated_by: Sean Larkin
---

# `aipm.toml` Engine Schema (today + affordances)

## Overview

The `engines` field already exists on `[package]` and parses into a typed
bitset, but no `aipm init`/wizard call site ever writes it. The TOML builder
accepts an `engines` slice yet five generation sites (init, two workspace_init
paths, two migrate emitter paths) all pass `engines: None`. Only the synthetic
starter plugin emits `engines = ["claude"]`. The lint and engine-validation
pipeline already consumes the field via a `MinimalManifest` shadow deserializer.

## 1. Top-level `Manifest` struct

File: `crates/libaipm/src/manifest/types.rs`

`Manifest` (`types.rs:11-43`):

```rust
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub package: Option<Package>,
    pub workspace: Option<Workspace>,
    pub dependencies: Option<BTreeMap<String, DependencySpec>>,
    pub overrides: Option<BTreeMap<String, String>>,
    pub components: Option<Components>,
    pub features: Option<BTreeMap<String, Vec<String>>>,
    pub environment: Option<Environment>,
    pub install: Option<Install>,
    pub catalog: Option<BTreeMap<String, String>>,
    pub catalogs: Option<BTreeMap<String, BTreeMap<String, String>>>,
}
```

`#[serde(deny_unknown_fields)]` on the top-level: any unknown key fails parse.

`Package` (`types.rs:46-88`):

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub description: Option<String>,

    #[serde(rename = "type")]
    pub plugin_type: Option<String>,

    pub files: Option<Vec<String>>,

    /// Engine compatibility list (e.g., `["claude", "copilot-cli"]`).
    /// `None` (field omitted) or `Some(EngineSet::empty())` (explicit
    /// empty list `engines = []`) means all engines.
    /// ...
    #[serde(default, deserialize_with = "engine_set_serde::deserialize")]
    pub engines: Option<EngineSet>,

    pub source: Option<SourceRedirect>,
}
```

The whole `Manifest` is **deserialize-only** — no `Serialize` derive. Writing
TOML happens through `toml_edit` in `manifest/builder.rs`.

## 2. Engine-related fields (verbatim)

`Package.engines` doc comment (`types.rs:65-83`):

```rust
    /// Engine compatibility list (e.g., `["claude", "copilot-cli"]`).
    /// `None` (field omitted) or `Some(EngineSet::empty())` (explicit
    /// empty list `engines = []`) means all engines.
    ///
    /// On disk this is stored as a TOML string array; in memory it is
    /// represented as an [`EngineSet`] bitflag set so callers can perform
    /// set-membership checks against `libaipm_engine_spec::EngineSet`
    /// directly. The TOML round-trip is handled by [`engine_set_serde`].
    ///
    /// **Validation:** if the manifest writes `engines = [...]` with a
    /// non-empty list whose entries are ALL unknown (no entry maps to an
    /// `Engine` variant), deserialization returns an error so the user's
    /// intended restriction isn't silently widened to "all engines". Mixed
    /// lists (some known + some unknown) drop the unknowns and keep the
    /// known bits.
    /// Engine names that are not recognised by the bundled engine schema
    /// are silently dropped on deserialize so manifests targeting future
    /// engines aipm doesn't yet know about still parse.
    #[serde(default, deserialize_with = "engine_set_serde::deserialize")]
    pub engines: Option<EngineSet>,
```

Custom deserialize adapter `engine_set_serde::deserialize` (`types.rs:323-357`):

```rust
mod engine_set_serde {
    use libaipm_engine_spec::{Engine, EngineSet};
    use serde::de::Error as DeError;
    use serde::{Deserialize, Deserializer};

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<Option<EngineSet>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw: Option<Vec<String>> = Option::deserialize(deserializer)?;
        let Some(names) = raw else {
            return Ok(None);
        };
        if names.is_empty() {
            return Ok(Some(EngineSet::empty()));
        }
        let mut set = EngineSet::empty();
        for name in &names {
            if let Some(engine) = Engine::from_name(name) {
                set |= engine.as_set();
            }
        }
        if set.is_empty() {
            let known: Vec<&'static str> = Engine::ALL.iter().map(|e| e.name()).collect();
            return Err(D::Error::custom(format!(
                "[package].engines = {names:?} contains no known engine names; \
                 valid names are {known:?} (unknown names are dropped, \
                 but at least one known name must remain)"
            )));
        }
        Ok(Some(set))
    }
}
```

Three semantic states for `engines`:

- field omitted → `None` ("all engines")
- `engines = []` → `Some(EngineSet::empty())` ("all engines")
- `engines = ["claude", ...]` → `Some(EngineSet)` bitset of recognized names;
  all-unknown rejects, mixed silently drops unknowns

`EngineSet` is a `bitflags` u32 generated by
`crates/libaipm-engine-spec/build.rs:189-211`. `Engine::from_name` and
`Engine::name` are the kebab-case round-trip pair generated at
`build.rs:151-187`. Currently-known engines per
`crates/libaipm-engine-spec/data/engine-api-schema.json:5-16`: `"claude"` and
`"copilot-cli"`. Bit constants: `EngineSet::CLAUDE`, `EngineSet::COPILOT_CLI`,
`EngineSet::ALL` (referenced at `manifest/mod.rs:574-577`).

There is **no `[engines]` table**, **no `[engines.claude]` sub-table**, and **no
top-level `engines` key on `Manifest` or `Workspace`** — only `Package.engines`
exists. There is also no `tooling`/`targets`/`tools` field. `Workspace`
(`types.rs:107-117`) carries no engine selector.

A second, parallel deserializer for the same field exists in
`crates/libaipm/src/engine.rs:101-111`
(`MinimalManifest`/`MinimalPackage`):

```rust
#[derive(serde::Deserialize, Default)]
struct MinimalPackage {
    #[serde(default)]
    engines: Option<Vec<String>>,
}

#[derive(serde::Deserialize, Default)]
struct MinimalManifest {
    #[serde(default)]
    package: Option<MinimalPackage>,
}
```

This one stays as `Vec<String>` (not `EngineSet`) and tolerates broken
manifests by treating them as universal — see `engine.rs:113-148`.

## 3. Loader / parser

File: `crates/libaipm/src/manifest/mod.rs`

Three entry points (`mod.rs:22-50`):

- `parse(toml_str: &str) -> Result<Manifest, Error>` — `toml::from_str` mapped
  to `Error::Parse`
- `parse_and_validate(toml_str, base_dir) -> Result<Manifest, Error>` — parses
  then runs `validate::validate`
- `load(fs, manifest_path) -> Result<Manifest, Error>` — reads file then
  `parse_and_validate` with `manifest_path.parent()` as `base_dir`

Validation rules — `crates/libaipm/src/manifest/validate.rs`:

`validate(manifest, base_dir)` (`validate.rs:127-162`):

- Calls `validate_package` if `manifest.package` is `Some`
- Calls `validate_dependencies` for `manifest.dependencies`
- Calls `validate_dependencies` for `manifest.workspace.dependencies` (lines
  141-145)
- Calls `validate_component_paths` only if `base_dir` is provided
- Multi-error wrapped in `Error::Multiple` (lines 154-161)

`validate_package` (`validate.rs:164-189`):

- `name` required + non-empty + matches `^(@[a-z0-9-]+/)?[a-z0-9][a-z0-9-]*$`
- `version` required + non-empty + parses via `semver::Version::parse`
- `plugin_type` if `Some(_)` must `parse::<PluginType>()`
- **No validation is run on `engines`.** The deserialize-time check in
  `engine_set_serde` is the only filter (`types.rs:339-353`); once parsing
  succeeds the value is accepted as-is. There is no `Error::InvalidEngine`
  variant in `manifest/error.rs`.

The `Error` enum (`crates/libaipm/src/manifest/error.rs:10-84`) variants:
`Parse`, `MissingField`, `InvalidName`, `InvalidVersion`,
`InvalidDependencyVersion`, `InvalidPluginType`, `InvalidWorkspaceProtocol`,
`ComponentNotFound`, `Io`, `Multiple(Vec<Self>)`. **No engine-related variant.**

## 4. Serializer / writer (init path)

TOML is written via `toml_edit::DocumentMut` in
`crates/libaipm/src/manifest/builder.rs`, not via serde. There is no symmetric
`Serialize` adapter.

`PluginManifestOpts` (`builder.rs:11-24`):

```rust
pub struct PluginManifestOpts<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub plugin_type: Option<&'a str>,
    pub description: Option<&'a str>,
    /// Optional declared engines list for `[package].engines`. `None` or an
    /// empty slice omits the field.
    pub engines: Option<&'a [&'a str]>,
}
```

`build_plugin_manifest(opts, components)` (`builder.rs:56-99`) writes fields in
this order, with `Option`/empty-slice gating:

1. `[package]`
2. `name = …`
3. `version = …`
4. `type = …` (only if `Some`)
5. `description = …` (only if `Some`)
6. `engines = […]` (only if `Some(slice)` AND `!slice.is_empty()`) —
   `builder.rs:72-80`:

```rust
if let Some(engines) = opts.engines {
    if !engines.is_empty() {
        let mut arr = Array::new();
        for e in engines {
            arr.push(*e);
        }
        pkg.insert("engines", value(arr));
    }
}
```

7. `[components]` table (only if `components` is `Some` and at least one inner
   array is non-empty)

`build_workspace_manifest` (`builder.rs:105-145`) writes `[workspace]` with
`members` then optional `plugins_dir`. **No `engines`, no `[package]`, no
engine-related affordance on the workspace path.**

**Five call sites** that pass into the builder, all currently set
`engines: None` except one:

1. `crates/libaipm/src/init.rs:158-179` — `aipm pack init` — `engines: None`
2. `crates/libaipm/src/workspace_init/mod.rs:144-168` —
   `generate_workspace_manifest` — calls `build_workspace_manifest` (no engines
   field exists on that path)
3. `crates/libaipm/src/workspace_init/mod.rs:276-299` —
   `generate_starter_manifest` — **the only writer that emits engines today**:
   `let starter_engines: &[&str] = &["claude"]; ... engines: Some(starter_engines)`
4. `crates/libaipm/src/migrate/emitter.rs:914-922` — migrate plugin emitter,
   `engines: None`
5. `crates/libaipm/src/migrate/emitter.rs:1136-1143` — migrate plugin emitter
   (second site), `engines: None`

The `aipm init` CLI flow is `cmd_init` in `crates/aipm/src/main.rs:398-445`
which builds `libaipm::workspace_init::Options` (`main.rs:413-422`). The
`Options` struct (`workspace_init/mod.rs:51-65`) carries `dir`, `workspace`,
`marketplace`, `no_starter`, `manifest`, `marketplace_name` — **no engines
field**.

Existing init wizard prompts (`crates/aipm/src/wizard.rs:23-73`,
`workspace_prompt_steps`):

1. "What would you like to set up?" — Marketplace only / Workspace only / Both
2. "Marketplace name:" — text input, default `"local-repo-plugins"`
3. "Include starter plugin?" — Confirm

**No engines prompt** and the `Init` clap variant at
`crates/aipm/src/main.rs:35-64` has no `--engine`/`--engines` flag.

## 5. `libaipm-engine-spec` threading hints

The `libaipm-engine-spec` crate (commit `14a7f4f`, PR #771, "feat: engine API
schema source-of-truth") landed the typed `Engine` enum and `EngineSet` bitset
that `Package.engines` already imports. Key threading points:

- `crates/libaipm-engine-spec/data/engine-api-schema.json:5-16` — on-disk source
  of engine names: `claude` (npm `@anthropic-ai/claude-code`) and `copilot-cli`
  (npm `@github/copilot`)
- `crates/libaipm-engine-spec/build.rs:151-211` — generates
  `pub enum Engine { Claude, CopilotCli }`, `Engine::ALL`, `Engine::name`,
  `Engine::from_name`, `Engine::as_set`, and `bitflags pub struct EngineSet: u32 { CLAUDE, COPILOT_CLI, ALL }`
- `crates/libaipm/src/lib.rs:44` re-exports:
  `pub use libaipm_engine_spec::{constraints, paths, Engine, EngineSet, MarketplaceHost};`
- `crates/libaipm/src/manifest/types.rs:6` imports `EngineSet` directly.
  `Package.engines` is `Option<EngineSet>`
- `crates/libaipm/src/manifest/mod.rs:560-630` test suite confirms three-state
  semantics:
  - `manifest_with_engines_field` — `engines = ["claude", "copilot-cli"]`
    parses to `EngineSet::CLAUDE | EngineSet::COPILOT_CLI`
  - `manifest_engines_field_with_only_unknown_names_fails_to_parse` —
    `engines = ["unknown-future-engine"]` errors with "contains no known engine
    names"
  - `manifest_engines_mixed_known_and_unknown_drops_unknowns` —
    `engines = ["claude", "future-engine"]` parses to just `CLAUDE`
  - `manifest_engines_explicit_empty_list_is_all_engines` —
    `engines = []` parses to `Some(EngineSet::empty())`

Adjacent threading where `engines` is already consumed (downstream of the
manifest):

- `crates/libaipm/src/lint/rules/valid_tool_name.rs` uses `EngineSet`/`Engine`
  from `libaipm_engine_spec` and queries the project's declared engines via
  `nearest_declared_engines` (line 63) — see lines 18, 105-116, 134-145
- `crates/libaipm/src/engine.rs:113-148` (`validate_via_manifest`) uses the
  alternate `MinimalManifest` shadow deserializer for plugin acquisition checks
- `crates/libaipm/src/installed.rs:32-38` carries a parallel
  `Plugin.engines: Vec<String>` (registry-side) with helpers `applies_to`,
  `effective_engines`, `engines_overlap` at lines 48-54 and 183-242

The companion JSON schema for tooling
(`schemas/aipm.toml.schema.json`) does not currently include an `engines`
property — only the `[workspace.lints]` block is constrained.

Related research and spec docs:

- `research/tickets/2026-05-01-510-aipm-toml-engines.md` — explicit ticket for
  #510 / #724 / #697 cross-cutting feature
- `research/docs/2026-05-04-engine-api-schema-source-of-truth.md` — companion
  research for the engine-spec crate (Non-Goal NG6 explicitly excludes adding
  the `[engines]` block in that PR, deferring to #510)
- `specs/2026-05-04-engine-api-schema-source-of-truth.md`
- `research/docs/2026-05-01-engine-tool-references.md`
- `research/docs/2026-05-02-engine-instructions-md-pattern-removal.md`

## 6. Sample / fixture `aipm.toml` files

**Workspace-root manifest** `aipm.toml`:

```toml
[workspace.lints.ignore]
paths = ["**/fixtures/**"]
```

No `engines`.

**Standalone plugin** `fixtures/standalone-plugin/aipm.toml`:

```toml
[package]
name = "my-standalone-skill"
version = "1.0.0"
type = "skill"
description = "A standalone skill with no workspace."
```

No `engines`.

**Other fixtures** — `fixtures/extension-test/aipm.toml`,
`fixtures/workspace-no-deps/.ai/hello-world/aipm.toml`,
`fixtures/workspace-separate-plugins-dir/.ai/{greeter,formatter}/aipm.toml`,
`fixtures/workspace-transitive-deps/.ai/{print-clock,get-current-time}/aipm.toml`,
and the workspace-root files. **None contain `engines`.**

**Schema test fixtures** at `schemas/tests/{valid.toml, valid-package-only.toml,
valid-with-dependencies.toml, invalid-unknown-rule.toml,
invalid-wrong-value-type.toml}` — none contain `engines`.

The **only `aipm.toml` actually emitted with an `engines` field today** is the
one written by `generate_starter_manifest` for the
`.ai/starter-aipm-plugin/aipm.toml` produced by
`aipm init --marketplace --manifest`. Verbatim engine list set in code
(`workspace_init/mod.rs:282`):

```rust
let starter_engines: &[&str] = &["claude"];
```

Resulting on-disk shape (per the round-trip test at
`workspace_init/mod.rs:429-454`) inserts `engines = ["claude"]` into
`[package]` after `description` and before `[components]`:

```toml
[package]
name = "starter-aipm-plugin"
version = "0.1.0"
type = "composite"
description = "Default starter plugin — scaffold new plugins, scan your marketplace, and log tool usage"
engines = ["claude"]

[components]
skills = ["skills/scaffold-plugin/SKILL.md"]
agents = ["agents/marketplace-scanner.md"]
hooks = ["hooks/hooks.json"]
scripts = ["scripts/scaffold-plugin.sh"]
```

**Inline test fixtures** (string literals in `crates/libaipm/src/manifest/mod.rs`)
that exercise `engines`:

- `mod.rs:564-569`: `engines = ["claude", "copilot-cli"]`
- `mod.rs:585-590`: `engines = ["unknown-future-engine"]` — expected to error
- `mod.rs:604-609`: `engines = ["claude", "future-engine"]`
- `mod.rs:620-625`: `engines = []`

**BDD feature fixtures** that reference the field as natural language:

- `tests/features/registry/mixed-sources.feature:32`: "a local plugin with
  aipm.toml declaring engines = [\"claude\"]"
- `tests/features/registry/engine-validation.feature:8`: "aipm.toml declaring
  engines = [\"claude\"]"
- `tests/features/registry/engine-validation.feature:13`: "aipm.toml declaring
  engines = [\"copilot\"]" (note: legacy `"copilot"` string, not the canonical
  `"copilot-cli"` per the engine-spec)
- `tests/features/lint/valid-tool-name.feature:20,32,38`: references
  `engines = ["claude"]` in messaging and given-clauses

## Code references

- `crates/libaipm/src/manifest/mod.rs`
- `crates/libaipm/src/manifest/types.rs`
- `crates/libaipm/src/manifest/validate.rs`
- `crates/libaipm/src/manifest/error.rs`
- `crates/libaipm/src/manifest/builder.rs`
- `crates/libaipm/src/init.rs`
- `crates/libaipm/src/workspace_init/mod.rs`
- `crates/libaipm/src/workspace_init/adaptors/mod.rs`
- `crates/libaipm/src/workspace_init/adaptors/claude.rs`
- `crates/libaipm/src/migrate/emitter.rs`
- `crates/libaipm/src/engine.rs`
- `crates/libaipm/src/installed.rs`
- `crates/libaipm/src/lint/rules/valid_tool_name.rs`
- `crates/libaipm/src/lib.rs`
- `crates/libaipm-engine-spec/src/types.rs`
- `crates/libaipm-engine-spec/src/lib.rs`
- `crates/libaipm-engine-spec/build.rs`
- `crates/libaipm-engine-spec/data/engine-api-schema.json`
- `crates/aipm/src/main.rs`
- `crates/aipm/src/wizard.rs`
- `crates/aipm/src/wizard_tty.rs`
- `research/tickets/2026-05-01-510-aipm-toml-engines.md`
- `research/docs/2026-05-04-engine-api-schema-source-of-truth.md`
- `specs/2026-05-04-engine-api-schema-source-of-truth.md`
- `schemas/aipm.toml.schema.json`
- `aipm.toml`
- `fixtures/standalone-plugin/aipm.toml`
- `fixtures/extension-test/aipm.toml`
- `tests/features/registry/engine-validation.feature`
- `tests/features/registry/mixed-sources.feature`
- `tests/features/lint/valid-tool-name.feature`
