---
date: 2026-05-01 23:30:00 UTC
researcher: Sean Larkin
git_commit: 0f4e837c0e3ba30ad34827197fd54c0c6a9a7348
branch: main
repository: aipm
topic: "`aipm.toml` engines property (#510) with engine-aware init wizard (#724) and `agent/valid-tool-name` lint rule (#697)"
tags: [research, tickets, issue-510, issue-724, issue-697, engines, manifest, init, lint, agents, multi-engine]
status: complete
last_updated: 2026-05-01
last_updated_by: Sean Larkin
---

# Research: `aipm.toml` Engines Field & Connected UX (#510, #724, #697)

## Research Question

Document the current state of `aipm` relevant to introducing an `engines` property in
`aipm.toml` ([#510](https://github.com/TheLarkInn/aipm/issues/510)), including (a) the
existing manifest schema and validation in `crates/libaipm/`, (b) the `aipm init`
wizard's engine-detection and folder-scaffolding flow
([#724](https://github.com/TheLarkInn/aipm/issues/724)), and (c) the existing lint rule
architecture and engine-aware tool reference data
([#697](https://github.com/TheLarkInn/aipm/issues/697)). Identify all touchpoints so
that an implementer can plan the cross-cutting feature with full awareness of
dependencies.

## The Three Issues at a Glance

| Issue | Title | What it asks for |
|---|---|---|
| [#510](https://github.com/TheLarkInn/aipm/issues/510) | `aipm.toml` has an `engines` property which enumerates the supported CLI's | Top-level `engines: Vec<String>` on `[package]`. Allowed values: `claude`, `copilot-cli`. Future: `gemini`, `codex`. Default = all engines supported. |
| [#724](https://github.com/TheLarkInn/aipm/issues/724) | `aipm init` should ask for engine in wizard prompts, and then follow the correct init folder | `aipm init` should prompt for which engines to support and only scaffold the relevant folders (don't always create `.claude/`). |
| [#697](https://github.com/TheLarkInn/aipm/issues/697) | `[lint] agent/valid-tool-name` | New lint rule that warns when an agent declares a tool that the project's `engines` don't all support (e.g. `WebFetch` is Claude-only; `read` is copilot-cli's canonical name). The rule is gated on the `engines` field. |

The three issues form a single feature thread: **#510 defines the schema field, #724
makes the wizard set it correctly, #697 consumes it.**

## Summary (TL;DR)

1. **The `engines` field already exists in the manifest struct.** `Package.engines:
   Option<Vec<String>>` is declared at
   [`crates/libaipm/src/manifest/types.rs:64-66`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/manifest/types.rs#L64-L66)
   with a passing test
   [`manifest_with_engines_field`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/manifest/mod.rs#L560-L573).
   The parse path round-trips. **What's missing for #510**: validation against an
   allowed-values set, emission in the manifest builder, generation in the five
   manifest-writing call sites, and integration with the existing `Engine` enum.
2. **Two distinct `Engine` enums exist today.**
   [`engine::Engine { Claude, Copilot }`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/engine.rs#L14-L21)
   is the manifest-side validator;
   [`discovery::types::Engine { Claude, Copilot, Ai }`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/discovery/types.rs#L39-L46)
   is the discovery/lint-side classifier (the `Ai` variant means "inside `.ai/`",
   not a real engine). Naming inconsistency: code uses `copilot`, the issue uses
   `copilot-cli`. Decision required before #510 lands.
3. **The init wizard never asks about engines and always creates `.claude/`.** The
   `ToolAdaptor`-based scaffolding has only one adaptor (`Claude`) at
   [`workspace_init/adaptors/mod.rs:13-15`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/workspace_init/adaptors/mod.rs#L13-L15)
   and it runs unconditionally when `--marketplace` is set. The seam for #724 is
   well-defined: extend the `defaults()` factory and add an engine prompt to the
   workspace wizard (mirroring the existing pattern in `aipm make plugin`).
4. **A precedent for engine semantics already exists** in
   [`installed::Plugin.engines`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/installed.rs#L32-L38)
   for global installs — `Vec<String>`, empty = all engines, with helpers
   `applies_to`, `effective_engines`, `engines_overlap`. The `[package].engines`
   field can reuse this semantic shape.
5. **The lint module is ready to receive a new agent rule but has no manifest
   access today.** The `Rule` trait at
   [`lint/rule.rs:16-41`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/lint/rule.rs#L16-L41)
   only takes `(file_path, fs)` — no `Manifest` is passed. The factory pattern at
   [`lint/rules/mod.rs:124-170`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/lint/rules/mod.rs#L124-L170)
   is where engine context would need to be threaded into the new rule, following
   the `instructions/oversized` configurable-rule pattern.
6. **The canonical `engine-api-schema.json` does not exist yet** — the
   `reverse-binary-analysis.md` workflow has not successfully populated it.
   Until it does, the manual tool catalog in
   [`research/docs/2026-05-01-engine-tool-references.md`](../docs/2026-05-01-engine-tool-references.md)
   is the lint rule's source of truth (10 strict-shared tools, 25 Claude-only,
   19 copilot-cli-only).

---

## Detailed Findings

### 1. The Engine Concept (Current State)

#### Two Engine enums

The codebase has two `Engine` enums that need to stay aligned:

- [`crates/libaipm/src/engine.rs:14-21`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/engine.rs#L14-L21)
  defines `pub enum Engine { Claude, Copilot }` with helpers `name() -> &str`,
  `all_names() -> &[&str]` (currently `&["claude", "copilot"]`),
  `marker_paths()`, and `marketplace_manifest_path()`. This is the engine identifier
  used by **plugin acquisition and registry semantics**.
- [`crates/libaipm/src/discovery/types.rs:39-46`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/discovery/types.rs#L39-L46)
  defines a *separate* `pub enum Engine { Claude, Copilot, Ai }` — the extra `Ai`
  variant represents "inside the `.ai/` marketplace root" and is used by the
  classification/lint pipeline. `lint::lint` checks `feature.engine == Engine::Ai`
  at [`lint/mod.rs:100`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/lint/mod.rs#L100).

#### Existing engine-name string usage

The string `"claude"` and `"copilot"` (sometimes `"both"`) appears in 14+ files. The
canonical names today are **`claude`** and **`copilot`** — *not* `copilot-cli` per
the #510 issue text. Notable hardcoded sites:

- [`engine.rs:51`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/engine.rs#L51) — `all_names() -> &["claude", "copilot"]`
- [`make/engine_features.rs:95-117`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/make/engine_features.rs#L95-L117) — `features_for_engine`, `validate_features` keyed on `"claude"`/`"copilot"`/`"both"`
- [`make/mod.rs:85`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/make/mod.rs#L85) — `if opts.engine == "claude" || opts.engine == "both"`
- [`crates/aipm/src/wizard.rs:342-346`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/aipm/src/wizard.rs#L342-L346) — `engine_from_index` returns `"claude" | "copilot" | "both"`
- [`crates/aipm/src/main.rs:1075,1146-1147`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/aipm/src/main.rs#L1075) — match arms
- [`migrate/unified.rs:203`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/migrate/unified.rs#L203) — hardcoded `[".claude", ".github"]`
- [`discovery/source.rs:47-49`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/discovery/source.rs#L47-L49) — directory→engine map (`.claude → Claude`, `.github → Copilot`, `.ai → Ai`)

**Decision point for #510**: keep `"copilot"` (back-compat with the entire codebase
plus the `Plugin.engines` registry) or migrate to `"copilot-cli"` (matches the
issue's wording and is more accurate now that GitHub also has copilot extensions for
VS Code, JetBrains, etc.). Recommend documenting this decision in #510 before
implementation.

#### Existing engine-array semantics (precedent)

[`installed::Plugin`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/installed.rs#L32-L38)
is the closest existing analogue:

```rust
pub struct Plugin {
    pub name: String,
    pub version: String,
    pub source: ResolvedSource,
    pub engines: Vec<String>,    // ← empty = all engines (the global-install precedent)
}
```

Helpers already in place:

- [`Plugin::applies_to(engine)`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/installed.rs#L48-L54) — empty Vec returns `true`
- [`Registry::install`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/installed.rs#L70-L97) — additive engine merge
- `effective_engines`, `engines_overlap`, `check_name_conflicts` —
  [`installed.rs:183-242`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/installed.rs#L183-L242)

The `[package].engines` field for #510 should reuse this exact semantic shape:
`Vec<String>`, empty (or `None`) = all engines.

### 2. Manifest Schema (#510 Touchpoints)

#### The field is already declared

```rust
// crates/libaipm/src/manifest/types.rs:64-66
/// Engine compatibility list (e.g., `["claude", "copilot"]`).
/// `None` or empty means all engines.
pub engines: Option<Vec<String>>,
```

[`Package`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/manifest/types.rs#L45-L70)
has `#[serde(deny_unknown_fields)]` so the field has to be declared on the struct
before any manifest can reference it — and it already is. The
[`manifest_with_engines_field`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/manifest/mod.rs#L560-L573)
test confirms `engines = ["claude", "copilot"]` round-trips.

#### What's missing for #510

| Concern | Current state | Action for #510 |
|---|---|---|
| Validation against allowed values | None — any string accepted | Add to [`validate_package`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/manifest/validate.rs#L164-L189). Recommend: validate against `engine::Engine::all_names()` so the enum stays the source of truth. |
| Emission in builder | [`PluginManifestOpts`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/manifest/builder.rs#L12-L37) has no `engines` field | Add `engines: Option<&'a [String]>` and call `pkg.insert("engines", ...)` in `build_plugin_manifest`. |
| Generation call sites (5) | None of them set `engines` | Update [`init.rs:168`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/init.rs), [`workspace_init/mod.rs:276,360`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/workspace_init/mod.rs), [`migrate/emitter.rs:845,1076`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/migrate/emitter.rs). |
| JSON Schema | [`schemas/aipm.toml.schema.json:5`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/schemas/aipm.toml.schema.json#L5) currently only validates `[workspace.lints]` per its description | Add an `engines` array property under `package` with the allowed-values enum. |
| Duplicate `MinimalManifest` deserializer | [`engine.rs:106-116`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/engine.rs#L106-L116) parses a tiny shadow struct to read `engines` early during acquisition | Decide whether to consolidate or keep separate (it exists to tolerate plugins with broken `aipm.toml`). |

#### Manifest pipeline reference

- Parse entry: [`manifest::parse`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/manifest/mod.rs#L22-L24), [`parse_and_validate`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/manifest/mod.rs#L34-L38), [`load`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/manifest/mod.rs#L46-L50)
- Validation: [`validate.rs:127-162`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/manifest/validate.rs#L127-L162)
- Error enum: [`manifest/error.rs:10-84`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/manifest/error.rs#L10-L84) — recommend adding `Error::InvalidEngine { value: String }` (modeled on `InvalidPluginType`).

#### Pattern from the historical `edition` field

[`research/docs/2026-03-26-edition-field-purpose-and-rationale.md`](../docs/2026-03-26-edition-field-purpose-and-rationale.md)
documents the prior pattern: `edition` was added as `Option<String>`, hardcoded to
`"2024"` in 5 generation paths, never validated, and later fully removed (per
`specs/2026-03-26-edition-field-semantics.md`). The lesson is to validate from day
one: `engines` should be checked against the `Engine` enum names so unknown values
don't silently slip through.

### 3. `aipm init` Wizard (#724 Touchpoints)

#### Today's flow has no engine prompt

The interactive wizard at
[`crates/aipm/src/wizard.rs:23-73`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/aipm/src/wizard.rs#L23-L73)
asks at most three questions:

1. **"What would you like to set up?"** — Marketplace only / Workspace only / Both
2. **"Marketplace name:"** — text input, defaults to `"local-repo-plugins"`
3. **"Include starter plugin?"** — yes/no

Engine selection is conspicuously absent. The
[`Init`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/aipm/src/main.rs#L35-L64)
clap variant has no `--engine` flag.

#### `.claude/` is created unconditionally

When `--marketplace` is on (the default), the only registered `ToolAdaptor`
(Claude) runs unconditionally:

```rust
// crates/libaipm/src/workspace_init/adaptors/mod.rs:13-15
pub fn defaults() -> Vec<Box<dyn ToolAdaptor>> {
    vec![Box::new(claude::Adaptor)]
}
```

The Claude adaptor at
[`adaptors/claude.rs:19-69`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/workspace_init/adaptors/claude.rs#L19-L69)
calls `fs.create_dir_all(&dir.join(".claude"))` and writes `.claude/settings.json`.
This is the root cause of #724's complaint: a copilot-only team gets a `.claude/`
folder they never use.

#### Existing engine-selection pattern (in `aipm make plugin`)

The `make plugin` wizard already has the exact UX #724 wants. See
[`crates/aipm/src/wizard.rs:297`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/aipm/src/wizard.rs#L297):

```rust
const ENGINE_OPTIONS: &[&str] = &["Claude Code", "Copilot CLI", "Both"];
```

with `engine_from_index` → `"claude" | "copilot" | "both"` and a CLI flag
`--engine` enforced via match arms at
[`main.rs:1075,1146-1147`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/aipm/src/main.rs#L1075).
**#724 should mirror this pattern** in the workspace init flow.

#### Folders/files created today (the suppression matrix)

[`research/docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md`](../docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md)
maps every generation path. For `aipm init`:

| Output | When | Suppressed by |
|---|---|---|
| `aipm.toml` (workspace) | `--workspace` | omitting `--workspace` |
| `.ai/.claude-plugin/marketplace.json` | `--marketplace` (default) | omitting `--marketplace` |
| `.ai/starter-aipm-plugin/.../*` | `--marketplace` && !`--no-starter` | `--no-starter` |
| `.ai/starter-aipm-plugin/aipm.toml` | `--marketplace` && !`--no-starter` && `--manifest` | omitting `--manifest` |
| `.claude/settings.json` | `--marketplace` (always) | nothing today — that's #724 |

#### Proposed seam for #724

1. Add an engine-selection prompt to `workspace_prompt_steps` (mirroring `ENGINE_OPTIONS`).
2. Add an `--engine` CLI flag on `Init` and a `MultiSelect` answer mapping in
   `wizard.rs::resolve_workspace_answers`.
3. Add a `Copilot` adaptor next to `claude::Adaptor` in
   [`workspace_init/adaptors/`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/workspace_init/adaptors/mod.rs)
   that writes `.github/copilot/` artifacts. The pre-refactor code that wrote
   Copilot config is described in
   [`research/docs/2026-03-19-init-tool-adaptor-refactor.md`](../docs/2026-03-19-init-tool-adaptor-refactor.md)
   — that doc's Open Question 3 is the seed for #724's design.
4. Pass the resolved engine list down so `defaults()` can return only the matching
   adaptors, and so the workspace `aipm.toml` gets `engines = [...]` written into
   the `[package]` section by `build_workspace_manifest` (today the workspace
   manifest has no `[package]` section — that may change with #510).
5. Update BDD scenarios at
   [`tests/features/manifest/workspace-init.feature`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/tests/features/manifest/workspace-init.feature)
   — particularly lines 75-85 which currently assert `.claude/settings.json` is
   created with `--no-starter`; that assertion needs to become engine-conditional.

### 4. Lint Module — `agent/valid-tool-name` (#697 Touchpoints)

#### Where the new rule slots in

```rust
// crates/libaipm/src/lint/rules/mod.rs:124-170 — the factory
pub(crate) fn quality_rules_for_kind(
    kind: &FeatureKind, config: &Config,
) -> Vec<Box<dyn Rule>> {
    match kind {
        // ...
        FeatureKind::Agent => vec![
            Box::new(agent_missing_tools::MissingTools),
            // ↑ add here: Box::new(agent_valid_tool_name::ValidToolName { ... })
        ],
        // ...
    }
}
```

The rule must also be appended to
[`catalog()`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/lint/rules/mod.rs#L178-L199)
so the LSP picks it up for completions/hover.

#### The `Rule` trait shape

```rust
// crates/libaipm/src/lint/rule.rs:16-41
pub trait Rule: Send + Sync {
    fn id(&self) -> &'static str;          // → "agent/valid-tool-name"
    fn name(&self) -> &'static str;
    fn default_severity(&self) -> Severity;
    fn help_url(&self) -> Option<&'static str> { None }
    fn help_text(&self) -> Option<&'static str> { None }
    fn check_file(&self, file_path: &Path, fs: &dyn Fs)
        -> Result<Vec<Diagnostic>, super::Error>;
}
```

Rules currently receive only `(file_path, fs)` — **no Manifest, no engine context.**

#### Two options for threading the engines list into the rule

**Option A — Configurable rule pattern (modeled on `instructions/oversized`).** The
rule struct carries the engines list as fields; the factory in
`quality_rules_for_kind` reads `aipm.toml` once and constructs the rule with the
resolved engines:

```rust
pub struct ValidToolName {
    pub engines: Vec<String>,  // empty = all engines
}
```

This requires the factory to either (a) parse the manifest itself or (b) accept a
`&Manifest` parameter from `lint::lint`. Today the factory takes only
`(&FeatureKind, &Config)`. Adding a `&Manifest` parameter is the cleanest extension
of the trait signature and keeps the rule pure.

**Option B — Use `Config::rule_options`.** Surface the engines list under
`[workspace.lints."agent/valid-tool-name".engines]`, parsed via the existing
`RuleOverride::Detailed { options }` path. This avoids any signature change but
duplicates the `[package].engines` value into the lint config — undesirable.

**Recommend Option A** — it preserves a single source of truth for the engines list.

#### Sibling rule (template to follow)

[`agent_missing_tools::MissingTools`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/lint/rules/agent_missing_tools.rs#L11-L69)
is the closest existing rule. It uses
[`read_agent_preamble`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/lint/rules/mod.rs#L101-L108)
to parse the agent file's frontmatter and inspects
`fm.fields.contains_key("tools")`. The new rule will go further: read the value of
`tools`, split on `,` (the frontmatter parser does NOT split — see
[`frontmatter.rs:39-145`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/frontmatter.rs#L39-L145)),
trim each tool name, and validate against the engines' tool catalogs.

#### Frontmatter shape (the input data)

`tools` is a comma-separated string. From an example agent file:

```yaml
---
name: reviewer
description: Reviews code changes
tools: Read, Grep, Glob, Bash
---
```

After parsing: `frontmatter.fields.get("tools") == Some("Read, Grep, Glob, Bash")`.
Line number: `frontmatter.field_lines.get("tools")`. Column-range helper:
[`frontmatter::field_value_range`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/frontmatter.rs#L168-L185).

#### Tool catalogs (the validation reference)

- **Authoritative future source**: `research/engine-api-schema.json` (does not exist yet)
- **Current snapshot**: [`research/docs/2026-05-01-engine-tool-references.md`](../docs/2026-05-01-engine-tool-references.md) (this PR)
  - 33 Claude built-in tools
  - 7 copilot-cli primary aliases + 16 compatible aliases + 2 MCP server prefixes
  - Strict-shared count: 10
  - Claude-only: 25, copilot-cli-only: 19

The lint rule should bake the catalog as a constant `&[(&str, &str)]` table or a
phf map. When `engine-api-schema.json` materializes, the rule's data table can be
swapped to a build-time inclusion of that JSON.

#### Other touchpoints

- Add a docs page at `docs/rules/agent/valid-tool-name.md` (matches the convention
  in [`docs/rules/`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/docs/rules/)).
- Register the rule ID in the JSON Schema's rule-id pattern at
  [`schemas/aipm.toml.schema.json:39`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/schemas/aipm.toml.schema.json#L39).
- Add a BDD scenario in
  [`tests/features/guardrails/quality.feature`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/tests/features/) (or wherever the `agent/missing-tools` BDD lives).

### 5. The `engine-api-schema.json` Workflow Status

[`/.github/workflows/reverse-binary-analysis.md`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/.github/workflows/reverse-binary-analysis.md)
defines a 120-minute weekly workflow that:

1. Installs each configured engine CLI via npm (`@anthropic-ai/claude-code`,
   `@github/copilot-cli`).
2. Reads the bundled/minified source.
3. Extracts manifest fields, settings paths, folder conventions,
   skill/command/agent registration, LSP config, MCP config, output styles,
   size limits, detection heuristics, discovery algorithm, validation rules, and
   **every internal tool-call name** the engine recognizes.
4. Diffs against the previous schema, generates suggestions, opens a PR.

The expected output shape (per the workflow's own description):

```jsonc
{
  "generated_at": "<ISO-8601>",
  "engines": [{ "name": "<engine>", "source": "npm", "package": "<pkg>" }],
  "versions": { "<engine>": "<installed-version>" },
  "apis": {
    "<engine>": {
      "manifest_fields": [...],
      "tool_calls": [
        { "name": "<tool>", "aliases": [...], "deprecated": false, "notes": "..." }
      ],
      // ...
    }
  },
  "tool_compatibility": {
    "shared_tools": ["<tool>"],
    "engine_exclusive_tools": {
      "<tool>": { "supported_by": ["<engine>"], "unsupported_by": ["<engine>"] }
    }
  }
}
```

**Status: file does not exist.** Neither
`research/engine-api-schema.json` nor
`research/engine-api-changelog.md` is committed. The workflow has not yet
successfully bootstrapped them. Until it does, the lint rule must use the manual
catalog in [`research/docs/2026-05-01-engine-tool-references.md`](../docs/2026-05-01-engine-tool-references.md).

---

## Code References (curated)

### Manifest & engines

- [`crates/libaipm/src/manifest/types.rs:64-66`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/manifest/types.rs#L64-L66) — `Package.engines: Option<Vec<String>>`
- [`crates/libaipm/src/manifest/mod.rs:560-573`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/manifest/mod.rs#L560-L573) — passing `manifest_with_engines_field` test
- [`crates/libaipm/src/manifest/validate.rs:164-189`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/manifest/validate.rs#L164-L189) — `validate_package` (no engine validation today)
- [`crates/libaipm/src/manifest/builder.rs:53-87`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/manifest/builder.rs#L53-L87) — `build_plugin_manifest` (does not emit engines)
- [`crates/libaipm/src/manifest/error.rs:10-84`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/manifest/error.rs#L10-L84) — error enum
- [`schemas/aipm.toml.schema.json:5,39`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/schemas/aipm.toml.schema.json#L5) — JSON schema (currently lints-only, plus rule-id pattern)
- [`crates/libaipm/src/engine.rs:14-21`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/engine.rs#L14-L21) — `Engine` enum (Claude, Copilot)
- [`crates/libaipm/src/engine.rs:51`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/engine.rs#L51) — `all_names()`
- [`crates/libaipm/src/engine.rs:106-152`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/engine.rs#L106-L152) — `MinimalManifest` shadow + `validate_via_manifest`
- [`crates/libaipm/src/discovery/types.rs:39-46`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/discovery/types.rs#L39-L46) — discovery `Engine { Claude, Copilot, Ai }`
- [`crates/libaipm/src/installed.rs:32-38`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/installed.rs#L32-L38) — `Plugin.engines` precedent
- [`crates/libaipm/src/installed.rs:48-54`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/installed.rs#L48-L54) — `applies_to`
- [`crates/libaipm/src/installed.rs:183-242`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/installed.rs#L183-L242) — engine merge / overlap helpers

### Init wizard

- [`crates/aipm/src/main.rs:35-64`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/aipm/src/main.rs#L35-L64) — `Init` clap variant (no `--engine` today)
- [`crates/aipm/src/main.rs:398-445`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/aipm/src/main.rs#L398-L445) — `cmd_init`
- [`crates/aipm/src/wizard.rs:23-73`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/aipm/src/wizard.rs#L23-L73) — `workspace_prompt_steps`
- [`crates/aipm/src/wizard.rs:78-131`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/aipm/src/wizard.rs#L78-L131) — `resolve_workspace_answers`
- [`crates/aipm/src/wizard.rs:140-150`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/aipm/src/wizard.rs#L140-L150) — `resolve_defaults`
- [`crates/aipm/src/wizard.rs:297,342-346`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/aipm/src/wizard.rs#L297) — existing `ENGINE_OPTIONS` and `engine_from_index` (in `make plugin` wizard)
- [`crates/aipm/src/wizard_tty.rs:37-51`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/aipm/src/wizard_tty.rs#L37-L51) — TTY bridge `resolve`
- [`crates/libaipm/src/workspace_init/mod.rs:96-120`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/workspace_init/mod.rs#L96-L120) — `init` (orchestrator)
- [`crates/libaipm/src/workspace_init/mod.rs:174-274`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/workspace_init/mod.rs#L174-L274) — `scaffold_marketplace`
- [`crates/libaipm/src/workspace_init/adaptors/mod.rs:13-15`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/workspace_init/adaptors/mod.rs#L13-L15) — `defaults()` (only Claude)
- [`crates/libaipm/src/workspace_init/adaptors/claude.rs:19-69`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/workspace_init/adaptors/claude.rs#L19-L69) — Claude adaptor (`ToolAdaptor` impl)
- [`tests/features/manifest/workspace-init.feature`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/tests/features/manifest/workspace-init.feature) — BDD scenarios (lines 75-85, 172-192 assert `.claude/settings.json` always exists)

### Lint module

- [`crates/libaipm/src/lint/rule.rs:16-41`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/lint/rule.rs#L16-L41) — `Rule` trait
- [`crates/libaipm/src/lint/rules/mod.rs:124-170`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/lint/rules/mod.rs#L124-L170) — `quality_rules_for_kind` factory
- [`crates/libaipm/src/lint/rules/mod.rs:178-199`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/lint/rules/mod.rs#L178-L199) — `catalog()`
- [`crates/libaipm/src/lint/rules/agent_missing_tools.rs:11-69`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/lint/rules/agent_missing_tools.rs#L11-L69) — sibling agent rule
- [`crates/libaipm/src/lint/rules/instructions_oversized.rs:25-138`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/lint/rules/instructions_oversized.rs#L25-L138) — configurable rule pattern
- [`crates/libaipm/src/lint/diagnostic.rs:6-63`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/lint/diagnostic.rs#L6-L63) — `Severity`, `Diagnostic`
- [`crates/libaipm/src/lint/config.rs:9-71`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/lint/config.rs) — `Config`, `RuleOverride`, `rule_options`
- [`crates/libaipm/src/lint/mod.rs:78-124`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/lint/mod.rs#L78-L124) — `run_rules_for_feature` (rule lifecycle)
- [`crates/libaipm/src/lint/rules/scan.rs:22-74`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/lint/rules/scan.rs#L22-L74) — `FoundAgent`, `read_agent`
- [`crates/libaipm/src/frontmatter.rs:13-185`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/libaipm/src/frontmatter.rs#L13-L185) — `Frontmatter` parser
- [`crates/aipm/src/main.rs:810-907`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/crates/aipm/src/main.rs#L810-L907) — `load_lint_config` (raw `toml::Value`, bypasses `Manifest`)

### Engine API workflow

- [`.github/workflows/reverse-binary-analysis.md`](https://github.com/TheLarkInn/aipm/blob/0f4e837c0e3ba30ad34827197fd54c0c6a9a7348/.github/workflows/reverse-binary-analysis.md) — workflow source
- `research/engine-api-schema.json` — does not exist yet
- `research/engine-api-changelog.md` — does not exist yet

---

## Architecture Documentation

### Manifest extension pattern (from the `edition` history)

The repo's pattern for adding an `Option<...>` field to `Package`:

1. Declare the field on the struct (`Package` has `deny_unknown_fields`, so this
   step is gating). **Already done for `engines`.**
2. Add validation in `validate_package` that maps the field to a domain enum and
   reports `Error::Invalid…` on bad values.
3. Add the field to the appropriate `*Opts` struct in `manifest::builder` and emit
   it inside `build_plugin_manifest` / `build_workspace_manifest` (uses
   `toml_edit`).
4. Update each generation call site (5 of them) to set the field with the right
   default.
5. Update the JSON Schema at `schemas/aipm.toml.schema.json`.
6. Add `manifest_with_<field>_field` round-trip test plus a validation test.

### Rule extension pattern (from `instructions/oversized` and the marketplace rules)

1. Add `<rule_name>.rs` under `crates/libaipm/src/lint/rules/`, struct + `impl Rule`.
2. Add `pub mod <rule_name>;` in `lint/rules/mod.rs:7-29`.
3. Add `Box::new(...)` in the appropriate arm of `quality_rules_for_kind` (line 124).
4. Add `Box::new(...)` in `catalog()` (line 178).
5. Document at `docs/rules/<category>/<name>.md`.
6. Register the rule ID in the JSON Schema rule-id enum (`schemas/aipm.toml.schema.json:39`).
7. Add a BDD scenario covering the rule.

### Init scaffolding extension pattern (from the existing `ToolAdaptor` trait)

1. Add `pub mod <engine>;` and a struct under
   `crates/libaipm/src/workspace_init/adaptors/`.
2. Implement `ToolAdaptor` (`name()` and `apply()`).
3. Add `Box::new(<engine>::Adaptor)` to `defaults()` — but **conditionally** for
   #724 based on the resolved engine list.
4. The CLI/wizard plumbs the engine list down into `defaults_for(&engines)` (a new
   variant of `defaults()`).

---

## Historical Context (from `research/`)

The 38 relevant prior research docs split into seven thematic groups (full
inventory in the locator agent's output). The most directly relevant — read these
before implementation:

### For #510 (engines field)

- [`research/docs/2026-04-06-plugin-system-feature-parity-analysis.md`](../docs/2026-04-06-plugin-system-feature-parity-analysis.md)
  — Microsoft APM precedent for engine semantics: additive engine updates,
  engine-scoped uninstall, name-conflict-by-engine. The `installed::Plugin.engines`
  shape was modeled on this.
- [`research/docs/2026-03-09-manifest-format-comparison.md`](../docs/2026-03-09-manifest-format-comparison.md)
  — Why `aipm.toml` is TOML and the friction of TOML in a JSON/YAML ecosystem.
- [`research/docs/2026-03-26-edition-field-purpose-and-rationale.md`](../docs/2026-03-26-edition-field-purpose-and-rationale.md)
  — The closest manifest-extension precedent. Lesson: validate from day one.
- [`research/docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md`](../docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md)
  — Map of every `aipm.toml`-emission site (5 paths). All five need updating to
  optionally emit `engines`.
- [`research/docs/2026-04-19-aipm-toml-editor-experience.md`](../docs/2026-04-19-aipm-toml-editor-experience.md)
  — JSON Schema and LSP completion expectations.

### For #724 (init wizard)

- [`research/docs/2026-03-19-init-tool-adaptor-refactor.md`](../docs/2026-03-19-init-tool-adaptor-refactor.md)
  — The refactor that introduced the `ToolAdaptor` trait. Pre-refactor code wrote
  `.vscode/settings.json` and `.copilot/mcp-config.json` (those were deleted).
  Open Question 3 ("Should the CLI auto-detect adaptors or accept a `--tool` flag?")
  is exactly the seam #724 is filling.
- [`research/docs/2026-03-22-rust-interactive-cli-prompts.md`](../docs/2026-03-22-rust-interactive-cli-prompts.md)
  — Library evaluation that landed `inquire`.
- [`research/docs/2026-03-25-marketplace-name-customization-in-init.md`](../docs/2026-03-25-marketplace-name-customization-in-init.md)
  — Precedent for adding a new prompt step to the workspace wizard.
- [`research/tickets/2026-04-14-0363-aipm-make-foundational-api.md`](../tickets/2026-04-14-0363-aipm-make-foundational-api.md)
  — Documents the exact `--engine` flag and `ENGINE_OPTIONS` wizard pattern that
  #724 should mirror.
- [`research/tickets/2026-04-14-0417-merge-pack-into-aipm.md`](../tickets/2026-04-14-0417-merge-pack-into-aipm.md)
  — `aipm make plugin` already has `--engine claude/copilot/both`.

### For #697 (lint rule)

- [`research/docs/2026-03-31-110-aipm-lint-architecture-research.md`](../docs/2026-03-31-110-aipm-lint-architecture-research.md)
  — Original lint architecture. The `Rule` trait and `Detector`-modeled design.
- [`research/docs/2026-04-02-aipm-lint-configuration-research.md`](../docs/2026-04-02-aipm-lint-configuration-research.md)
  — How `[workspace.lints]` works. Rules don't see manifest data today.
- [`research/docs/2026-04-07-lint-rules-287-288-289-290.md`](../docs/2026-04-07-lint-rules-287-288-289-290.md)
  — Most recent precedent for adding new lint rules.
- [`research/tickets/2026-04-11-185-prevent-long-instructions-files.md`](../tickets/2026-04-11-185-prevent-long-instructions-files.md)
  — The configurable-rule pattern (`instructions/oversized`).
- [`research/docs/2026-03-28-copilot-cli-migrate-adapter.md`](../docs/2026-03-28-copilot-cli-migrate-adapter.md)
  — Per-engine alias mapping (most relevant for the rule's data table).
- [`research/docs/2026-03-31-cli-binary-frontmatter-hook-analysis.md`](../docs/2026-03-31-cli-binary-frontmatter-hook-analysis.md)
  — Binary-derived ground truth for Claude v2.1.87 + copilot-cli v1.0.12.
- [`research/docs/2026-03-28-copilot-cli-source-code-analysis.md`](../docs/2026-03-28-copilot-cli-source-code-analysis.md)
  — Direct source reading of `app.js` (Zod schemas, hook events, MCP).

### For the engine concept (cross-cutting)

- [`research/docs/2026-05-01-github-copilot-skills-migrate-lint-silent-failure.md`](../docs/2026-05-01-github-copilot-skills-migrate-lint-silent-failure.md)
  — Documents that lint never reads `[package].engines` from `aipm.toml` today;
  marker-files map; the gap that #697 closes.
- [`research/docs/2026-04-12-dry-rust-architecture-audit.md`](../docs/2026-04-12-dry-rust-architecture-audit.md)
  — `ToolAdaptor` is only used during init; no engine-specific adapter exists for
  install/link/lint. Frames the abstraction gap that #510/#697 widen.
- [`research/feature-list.json`](../feature-list.json) and
  [`research/progress.txt`](../progress.txt) — In-flight engine enum + adapter
  work tracking.

---

## Cross-Cutting Implementation Notes

### Naming canonicalization (decision required)

The codebase consistently uses `"copilot"` as the engine name string (in
`Engine::all_names()`, `make_plugin --engine copilot`, BDD scenarios, registry
plugin entries). Issue #510 says `copilot-cli`. Three options:

1. **Keep `"copilot"`.** Lowest churn. Update #510's prose to use `copilot` and
   document `copilot-cli` as a future alias if/when GitHub Copilot expands beyond
   the CLI.
2. **Migrate to `"copilot-cli"`.** Higher accuracy as the AI-tools ecosystem
   diversifies. Requires updating ~14 files plus existing registries on disk.
   Add an alias pass to migrate older lockfiles.
3. **Accept both.** `Engine::from_str` accepts `"copilot"` and `"copilot-cli"` →
   `Copilot`. Persist canonical form in writing. Lowest user-friction.

Recommend **option 3** for the lint rule and validator, with **option 1** for
emission (preserves on-disk consistency).

### Where `engines` could live in the manifest

`Package.engines` already exists. But for **workspace-level** projects (where
`[package]` may be absent and `[workspace]` present instead), there's no engines
field. #724's init wizard for `--workspace --marketplace` writes a workspace-only
manifest today. To make `engines` queryable for workspace projects, either:

- Add `engines` to `Workspace` as well (mirrors `dependencies` pattern), OR
- Always emit a `[package]` section in the workspace manifest with at least `name`,
  `version`, and `engines`.

The lint rule needs a single answer to "what engines does this project target?"
that works for both shapes.

### What an implementer would touch (per issue)

| File | #510 | #724 | #697 |
|---|:---:|:---:|:---:|
| `crates/libaipm/src/manifest/types.rs` | – (already declared) | maybe (workspace engines) | – |
| `crates/libaipm/src/manifest/validate.rs` | **add** validation | – | – |
| `crates/libaipm/src/manifest/builder.rs` | **add** emission | maybe (workspace) | – |
| `crates/libaipm/src/manifest/error.rs` | **add** `InvalidEngine` | – | – |
| `crates/libaipm/src/init.rs` | emit engines | – | – |
| `crates/libaipm/src/workspace_init/mod.rs` | emit engines | **conditional adaptors** | – |
| `crates/libaipm/src/workspace_init/adaptors/mod.rs` | – | **`defaults_for(&engines)`** | – |
| `crates/libaipm/src/workspace_init/adaptors/copilot.rs` (new) | – | **new file** | – |
| `crates/libaipm/src/migrate/emitter.rs` | emit engines (×2) | – | – |
| `crates/aipm/src/main.rs` | – | **`Init::engine` flag** | – |
| `crates/aipm/src/wizard.rs` | – | **engine prompt** | – |
| `crates/aipm/src/wizard_tty.rs` | – | engine resolve | – |
| `crates/libaipm/src/lint/rule.rs` | – | – | maybe (`&Manifest`) |
| `crates/libaipm/src/lint/rules/mod.rs` | – | – | **factory + catalog** |
| `crates/libaipm/src/lint/rules/agent_valid_tool_name.rs` (new) | – | – | **new file** |
| `docs/rules/agent/valid-tool-name.md` (new) | – | – | **new file** |
| `schemas/aipm.toml.schema.json` | **add** engines schema | – | **add** rule-id |
| `tests/features/manifest/workspace-init.feature` | – | **scenarios** | – |
| `tests/features/guardrails/quality.feature` | – | – | **scenarios** |

### Suggested implementation order

1. **#510 first.** Validation, emission, JSON schema, generation in 5 sites.
   Land alone — touches the most files but is structurally simple. Ship the
   `InvalidEngine` error variant.
2. **#697 second.** Builds on #510's validated `engines` field. Decide
   `Rule::check_file` signature (whether to add `&Manifest`) and lock the data
   table from the manual snapshot. The rule can ship before
   `engine-api-schema.json` materializes.
3. **#724 third.** Most user-facing. Adds the new `Copilot` adaptor (the
   pre-existing `2026-03-19` refactor took the previous Copilot scaffolding code
   out — it's a new implementation, not a revival, since the v1 was VS Code-only).
   Drives the new `engines` field with the wizard's selection.

This ordering avoids implementing #697 against a moving validation target and
avoids #724 emitting engine values that #510's validation might reject.

---

## Open Questions

1. **Engine name canonicalization.** Should the canonical engine name be
   `"copilot"` (current code) or `"copilot-cli"` (issue text)? Affects 14+ files
   and persisted lockfiles.
2. **Workspace-level engines.** Does `engines` belong only on `[package]`, or
   also on `[workspace]`? The lint rule needs to answer "what engines does this
   project target?" for workspace-only manifests.
3. **`Rule::check_file` signature change.** Should the trait accept `&Manifest`
   for #697, or should the engines list be threaded via the configurable-rule
   pattern (`Config::rule_options`)? The former is cleaner; the latter avoids
   trait churn.
4. **`agent/valid-tool-name` data freshness.** Until `engine-api-schema.json`
   exists, the lint's tool catalog is the manual snapshot. How is staleness
   surfaced — `tracing::warn` on the lint command? A separate check?
5. **Two `Engine` enums alignment.** Should `engine::Engine` and
   `discovery::types::Engine` be unified? The `Ai` variant is awkward in the
   manifest context. Currently `Ai` means "in `.ai/`" (a marketplace-scope
   marker), not a real engine.
6. **Existing `.claude/settings.json` for non-Claude projects.** What happens to
   the `.claude/` folder in a project that ran `aipm init` *before* #724 ships
   and then later changes `engines` to copilot-only? Should `aipm migrate` or
   `aipm lint` flag the now-orphaned folder?
7. **Plugin engines vs project engines interaction.** `Plugin.engines` (the
   global-install registry) and `Package.engines` (manifest) have the same
   shape but different scopes. Should `aipm install` warn if a dependency's
   `engines` is disjoint from the project's `engines`? (Potential follow-up
   issue.)

---

## Related Research

- [`research/docs/2026-05-01-engine-tool-references.md`](../docs/2026-05-01-engine-tool-references.md)
  — Tool catalog comparison written alongside this ticket; the lint rule's
  source data until `engine-api-schema.json` exists.
- [`research/docs/2026-04-06-plugin-system-feature-parity-analysis.md`](../docs/2026-04-06-plugin-system-feature-parity-analysis.md)
- [`research/docs/2026-03-31-110-aipm-lint-architecture-research.md`](../docs/2026-03-31-110-aipm-lint-architecture-research.md)
- [`research/docs/2026-03-19-init-tool-adaptor-refactor.md`](../docs/2026-03-19-init-tool-adaptor-refactor.md)
- [`research/docs/2026-03-26-edition-field-purpose-and-rationale.md`](../docs/2026-03-26-edition-field-purpose-and-rationale.md)
- [`research/docs/2026-03-28-copilot-cli-migrate-adapter.md`](../docs/2026-03-28-copilot-cli-migrate-adapter.md)
- [`research/docs/2026-03-31-cli-binary-frontmatter-hook-analysis.md`](../docs/2026-03-31-cli-binary-frontmatter-hook-analysis.md)
- [`research/docs/2026-03-28-copilot-cli-source-code-analysis.md`](../docs/2026-03-28-copilot-cli-source-code-analysis.md)
- [`research/docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md`](../docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md)
- [`research/tickets/2026-04-14-0363-aipm-make-foundational-api.md`](../tickets/2026-04-14-0363-aipm-make-foundational-api.md)
- [`research/tickets/2026-03-28-110-aipm-lint.md`](../tickets/2026-03-28-110-aipm-lint.md)
- [`research/tickets/2026-04-11-185-prevent-long-instructions-files.md`](../tickets/2026-04-11-185-prevent-long-instructions-files.md)
