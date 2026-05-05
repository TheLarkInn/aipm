---
date: 2026-05-04 16:08:44 UTC
researcher: Sean Larkin (selarkin@microsoft.com)
git_commit: 00f7bcdbec608c3a33e730cf233119d1340cc578
branch: main
repository: aipm
topic: "Treating research/engine-api-schema.json as the source of API truth: codegen, validation, and consumer wiring"
tags: [research, codebase, engine-api-schema, reverse-binary-analysis, codegen, typify, schemars, build.rs, lazy_lock, phf, valid-tool-name, lint, manifest, multi-engine]
status: complete
last_updated: 2026-05-04
last_updated_by: Sean Larkin
---

# Research

## Research Question

> Now that we have an agentic workflow doing reverse binary analysis and writing `research/engine-api-schema.json`, we need to treat this file as the source of API truth — or build a system that turns it into one. Today the data is unstructured. The workflow updates this file once a week and we need it to actually update real schemas / type signatures / etc. What concrete codegen and consumer-wiring mechanisms exist, fit, or could be introduced — covering engine detection, manifest validation, lint rules (`valid-tool-name`), and adaptor logic — given the current architecture?

## Summary

`research/engine-api-schema.json` is currently a **prose-described JSON artifact** rewritten weekly by a Copilot CLI–driven GitHub Agentic Workflow (`reverse-binary-analysis.md`). It is *not yet* read by any Rust code in the workspace. Today, every fact it captures is **independently re-encoded by hand** in `crates/libaipm` as inline `&[&str]` slices, `match` arms over an `Engine` enum, free-floating string literals, and per-engine detector files. There are **two divergent `Engine` enums** (`engine.rs::Engine` with `{Claude, Copilot}` and `discovery/types.rs::Engine` with `{Claude, Copilot, Ai}`), plus a third string-keyed engine model in `make/engine_features.rs`. The `valid-tool-name` lint (issue #697) does not yet exist; its data dependency is exactly what the schema's `tool_compatibility.engine_exclusive_tools` provides.

The repo has **zero existing codegen infrastructure**: no `build.rs`, no `include_str!`, no `phf`, no `typify`, no `schemars`, no `lazy_static`/`once_cell` (only one stdlib `OnceLock` use in `lint/config.rs`). It is a "plain serde + stdlib" workspace. So whatever pattern is chosen will be a **first-of-its-kind addition** — the choice will set the precedent for future generated tables.

The strongest end-to-end pattern surfaced by external research is a **hybrid layered approach**: hand-author a JSON Schema (the meta-schema) that the agent's output must conform to, generate Rust types from that meta-schema via `typify` in a `build.rs`, validate each weekly regen against the meta-schema in CI via `jsonschema`, embed the data file with `include_str!`, parse once via `LazyLock<EngineApiSchema>`, and feed `phf_codegen` from the same `build.rs` to make hot lookups (e.g. `valid-tool-name`) zero-cost. This is the recipe Oxide Computer's `oxide.rs` SDK effectively follows.

Three things must happen *before* codegen is wired in to make the source-of-truth claim meaningful:

1. The agent's output shape must be **frozen as a JSON Schema** rather than described only in prose inside `reverse-binary-analysis.md`. Without that, weekly regens will silently shift shape and break any consumer.
2. The two `Engine` enums must converge (or the second one must be derivable from the first via discovery-only refinement). Otherwise codegen has two targets.
3. The schema's per-engine fields need to be **promoted from "documentation" to "consumable"** — several keys are currently free-text prose (`detection_heuristics`, `discovery_algorithm` are `string[]` of natural-language sentences, not machine-actionable). These either need machine-typed equivalents or a clear separation between prose and data.

## Detailed Findings

### 1. The schema artifact today

**File**: [`research/engine-api-schema.json`](../engine-api-schema.json) (455 lines, ~32 KB) — last regenerated 2026-05-01 covering `claude@2.1.126` and `copilot-cli@1.0.40`.

**Top-level shape** (de facto, not formally specified):
```jsonc
{
  "generated_at": "ISO-8601",
  "engines":   [{ "name", "source", "package" }],   // bootstrap list
  "versions":  { "<engine>": "semver" },
  "apis":      { "<engine>": EngineApi },           // per-engine surface
  "tool_compatibility": {
    "shared_tools":          [string],
    "engine_exclusive_tools": { "<tool>": { "supported_by":[…], "unsupported_by":[…] } }
  },
  "suggestions": {
    "<engine>": { "adaptor_fixes":[…], "test_cases":[…], "behaviour_variants":[…] }
  }
}
```

**`EngineApi` keys** (vary in machine-actionability):

| Key | Shape | Machine-actionable? |
|---|---|---|
| `manifest_fields` | `string[]` (free-text descriptions) | **Partial** — `claude` is empty, `copilot-cli` mixes type info into prose like `"name (max 64 chars, regex /^[a-zA-Z0-9-]+$/)"` |
| `manifest_search_paths` | `string[]` | **Yes** |
| `settings_paths` | `string[]` | **Yes** |
| `folder_conventions` | `string[]` | **Yes** |
| `convention_files` | `[{ filename, convention_paths }]` | **Yes** |
| `skill_registration` / `mcp_config` / `lsp_config` | structured object | **Partial** — mixes structured fields with `notes` prose |
| `output_styles` | `string[]` (currently empty for both) | **Yes** |
| `size_limits` | `{ <key>: number, notes: string }` | **Yes** for numeric keys |
| `detection_heuristics` | `string[]` of NL sentences | **No** — prose-only |
| `discovery_algorithm` | `string[]` of NL sentences | **No** — prose-only |
| `rules` | `string[]` of NL sentences | **No** — prose-only |
| `tool_calls` | `[{ name, aliases, deprecated, notes }]` | **Yes** |
| `agent_commands` | `string[]` | **Yes** |
| `feature_flags` | `string[]` | **Yes** |

This mix is the central design tension: half the schema is consumable, half is documentation. A meta-schema needs to either normalize prose fields into structured equivalents (e.g. `detection_heuristics: [{ kind: "file_exists", path: "CLAUDE.md" }]`) or explicitly partition `data` vs `notes`.

### 2. How the schema is generated (`reverse-binary-analysis.md`)

**Source**: [`.github/workflows/reverse-binary-analysis.md`](../../.github/workflows/reverse-binary-analysis.md) (303 lines) compiled to `.lock.yml` (1302 lines) via `gh aw compile`.

**Trigger**: weekly `cron: "36 12 * * 2"` (Tuesday 12:36 UTC) + `workflow_dispatch`. Timeout 120 min ([CLAUDE.md:83](../../CLAUDE.md)).

**Engine driver**: GitHub Copilot CLI v1.0.35 with model `claude-sonnet-4.6` ([lock.yml:714](../../.github/workflows/reverse-binary-analysis.lock.yml)).

**Inputs**:
- Reads existing `research/engine-api-schema.json` for the `engines` and `versions` map (md:86–90).
- For each engine, runs `npm install --prefix /tmp/rba-engines/<engine> --ignore-scripts <package>@latest` (md:104), then locates the entry-point JS via `package.json` `"main"` and `find` (md:118–126).

**Per-engine extraction** (md:128–164): manifest fields, settings paths, folder conventions, skill/command/agent registration, LSP/MCP config, output styles, size limits, detection heuristics, discovery algorithm, rules, internal tool calls (`{ name, aliases, deprecated, notes }`), agent commands, feature flags.

**Diff + suggestions** (md:166–180): added/removed/changed fields against prior schema; suggested adaptor fixes, test cases, and behaviour variants per engine.

**Cross-engine analysis** (md:182–206): produces `shared_tools` and `engine_exclusive_tools` — *the exact data shape the `valid-tool-name` lint (#697) needs.*

**Outputs**:
- `research/engine-api-schema.json` — overwritten in place.
- `research/engine-api-changelog.md` — dated table prepended (newest first), format `| Field | Change |` with `Added / Removed / Changed (was: ..., now: ...)` rows (md:260–266).

**PR creation** (`safe-outputs`, md:25–36):
- One PR per run, branch `reverse-binary-analysis/<date>`, title `[reverse-binary-analysis] API schema update <date>`, labels `automation, analysis`, draft=false, max_patch_size 1024 KB.
- `noop` if `git diff --stat` shows the two files unchanged (md:281–284).
- Protected paths include `.github/`, `.agents/`, `.githooks/`, `.husky/` plus assorted lockfiles — the agent **cannot modify `crates/`** ([lock.yml:423](../../.github/workflows/reverse-binary-analysis.lock.yml)).

**Critical gap**: the expected JSON shape lives **only as fenced JSONC examples in the prompt** (md:74–82, 196–203, 212–254). There is no `.schema.json` file, no TypeScript interface, no Rust type. The agent is constrained by prose + the prior committed file — meaning **silent shape drift is possible on any regen**. PR #738 (`eb09ac0`) established the current baseline.

### 3. How engine information is represented in code today

**Two coexisting `Engine` enums:**

- [`crates/libaipm/src/engine.rs:13-21`](../../crates/libaipm/src/engine.rs) — `pub enum Engine { #[default] Claude, Copilot }` with `const fn` methods `marker_paths()`, `marketplace_manifest_path()`, `name()`, `all_names()`. Each method is a `match` over variants returning string literals. Adding a new engine = new variant + new arm in 4 methods.
- [`crates/libaipm/src/discovery/types.rs:38-46`](../../crates/libaipm/src/discovery/types.rs) — `pub enum Engine { Claude, Copilot, Ai }` (no `Default`, three variants). The extra `Ai` variant models `.ai/` as a marketplace host root.
- [`crates/libaipm/src/make/engine_features.rs:95-101`](../../crates/libaipm/src/make/engine_features.rs) — third representation: `engine: &str` matched against `"claude" | "copilot" | "both"`.

**Engine detection algorithm** lives in two parallel implementations that scan path components for `.claude` / `.github` / `.ai`:
- [`discovery/source.rs:37-54`](../../crates/libaipm/src/discovery/source.rs) — typed walker via `Path::ancestors()`, returns `Option<(Engine, PathBuf)>`.
- [`lint/rules/scan.rs:33-44`](../../crates/libaipm/src/lint/rules/scan.rs) — string-typed walker returning `&'static str`.
- [`engine.rs:155-157`](../../crates/libaipm/src/engine.rs) — plugin-marker validation iterates `Engine::marker_paths()` calling `Path::exists`.

Plus [`discovery_legacy.rs:48-71`](../../crates/libaipm/src/discovery_legacy.rs) — a pattern-driven directory walker accepting `&[&str]` of literal directory names.

**Convention-file detection** is the only centrally-listed table:
```rust
// crates/libaipm/src/discovery/instruction.rs:32-39
pub const INSTRUCTION_FILENAMES: &[&str] = &[
    "claude.md", "agents.md", "copilot.md",
    "instructions.md", "gemini.md", "copilot-instructions.md",
];
```
Matched case-insensitively (`instruction.rs:57`, `:72-75`) with an additional `*.instructions.md` suffix shape.

**Tool-name awareness** lives in [`crates/libaipm/src/lint/rules/known_events.rs`](../../crates/libaipm/src/lint/rules/known_events.rs):
- `CLAUDE_EVENTS: &[&str]` — 27 PascalCase strings (lines 10–38).
- `COPILOT_EVENTS: &[&str]` — 10 camelCase strings (lines 41–52).
- `COPILOT_LEGACY_MAP: &[(&str, &str)]` — 10 `(legacy, canonical)` tuples (lines 58–69).

The doc comment names the source of truth file (`research/docs/2026-03-31-cli-binary-frontmatter-hook-analysis.md`) and instructs maintainers to "re-edit the constant when binary analysis is re-run." This is the **closest existing analog** to consuming `engine-api-schema.json` typed-and-static — but updated by hand.

`is_valid_event` (`known_events.rs:75-84`) dispatches on `&str` engine root: `".claude" => CLAUDE_EVENTS.contains(&e), ".github" => COPILOT_EVENTS.contains(&e) || legacy_map.iter().any(...)`.

**Manifest validation** ([`crates/libaipm/src/manifest/validate.rs`](../../crates/libaipm/src/manifest/validate.rs), [`types.rs`](../../crates/libaipm/src/manifest/types.rs)) is hand-rolled with no validation crate:
- Name regex (`is_valid_segment`, validate.rs:92–103) is a byte-by-byte ASCII walk; doc-comment names the equivalent regex.
- Skill name max 64 ([`lint/rules/skill_name_too_long.rs:13`](../../crates/libaipm/src/lint/rules/skill_name_too_long.rs)).
- Description max 1024 ([`skill_desc_too_long.rs:13`](../../crates/libaipm/src/lint/rules/skill_desc_too_long.rs)).
- Skill name regex (`skill_name_invalid.rs:13-24`) — manual byte iteration replicating Copilot Zod regex.
- Skill char budget 15,000 (`skill_oversized.rs:13`).
- Instructions max lines 100, max chars 15,000 (`instructions_oversized.rs:20-22`).
- Version: `semver::VersionReq::parse` / `Version::parse` (validate.rs:118, :179).
- Schema typing: serde `#[derive(Deserialize)]` with `#[serde(deny_unknown_fields)]` ([types.rs:11, :46](../../crates/libaipm/src/manifest/types.rs)). No `schemars` derives.

**Per-engine branch sites** (representative, not exhaustive):

| Site | Branch shape | Encoded data |
|---|---|---|
| `engine.rs:25-32` | `match self { Claude => &[…], Copilot => &[…] }` | Marker file paths |
| `engine.rs:35-40` | same | Marketplace manifest path (`.claude-plugin/marketplace.toml` vs `.github/plugin/marketplace.toml`) |
| `discovery/source.rs:46-50` | `match name { ".claude"=>Claude, ".github"=>Copilot, ".ai"=>Ai }` | Folder name → engine |
| `discovery/layout.rs:52` | `if !(engine == Copilot && grandparent_is_copilot)` | Copilot-only `<root>/copilot/<name>/SKILL.md` accommodation |
| `migrate/adapters/agent.rs:33-34, 91-92` | `feat.engine == Copilot && feat.kind == Agent` | Adapter-applies-to predicate |
| `migrate/adapters/agent.rs:58, ~108` | `"${COPILOT_AGENT_DIR}/"` vs `"${CLAUDE_AGENT_DIR}/"` | Per-engine script reference template |
| `lint/rules/known_events.rs:76-83` | `match tool { ".claude"=>…, ".github"=>… }` | Hook event vocabulary |
| `make/engine_features.rs:96-101` | `match engine { "claude"=>…, "copilot"=>…, "both"=>… }` | Per-engine feature support |
| `aipm/src/main.rs:1074-1077` | `match resolved_engine.as_str() { "claude"\|"copilot"\|"both" => {} }` | CLI engine string validation |

**Folder/file path literals are scattered** — `.claude-plugin`, `.github/plugin`, `marketplace.json`, `marketplace.toml`, `plugin.json`, `aipm.toml`, `settings.json` each appear at 5–15 unrelated sites without a centralized constant module. (See the codebase-analyzer report for the full list.)

### 4. Existing codegen / static-data patterns in the workspace

| Pattern | Workspace usage |
|---|---|
| `build.rs` files | **None.** Workspace has zero build scripts. |
| `include_str!` / `include_bytes!` | **None.** No usage anywhere. |
| `OnceLock` / `LazyLock` / `lazy_static` / `once_cell` | **One** — function-local `OnceLock<BTreeMap>` in [`lint/config.rs:64-69`](../../crates/libaipm/src/lint/config.rs) for an empty-fallback map. No globals. |
| `serde_json::from_str` for embedded JSON | **None embedded.** All usage is runtime, file-backed (registry index, cache, generate/marketplace, lint/migrate untyped `Value` parsing). |
| `&[&str]` / `&[(&str, &str)]` static slices | **Pervasive.** Closest analog to schema-driven tables — see `known_events.rs`, `instruction.rs`, `walker.rs::SKIP_DIRS`, `spec.rs::SOURCE_PREFIXES`, `lint/mod.rs::RECOGNIZED_SOURCE_NAMES`, `wizard.rs::ENGINE_OPTIONS`, `main.rs::SUPPORTED_SOURCES`, etc. All hand-edited, all consumed via linear scan. |
| `schemars` / `typify` / `prost-build` / `tonic-build` / `oapi-codegen` | **None.** Workspace dependencies are: `serde`, `serde_json` (with `preserve_order`), `thiserror`, plus stdlib. No JSON-Schema codegen crate is in the dependency closure. |
| `rust-embed` / `include_dir` | **None.** |
| `macro_rules!` / proc-macros | **None defined.** Only `#[derive(...)]` consumption. |

The workspace is therefore a **green-field site for codegen**. Whichever pattern is chosen will set the precedent.

### 5. The `valid-tool-name` lint (issue #697) — the most direct consumer

`valid-tool-name` does not yet exist in the codebase. References to it appear only in research:
- [`research/engine-api-changelog.md:34, 42-43`](../engine-api-changelog.md)
- [`research/docs/2026-05-01-engine-tool-references.md:7, 18-21, 204`](2026-05-01-engine-tool-references.md)
- [`research/tickets/2026-05-01-510-aipm-toml-engines.md`](../tickets/2026-05-01-510-aipm-toml-engines.md) — multiple lines tying the lint to the `[engines]` block in `aipm.toml`
- The agent prompt in [`reverse-binary-analysis.md:44, 62, 182, 190, 205, 300`](../../.github/workflows/reverse-binary-analysis.md) — the agent is *explicitly told* its `tool_compatibility` output should drive the lint

The data needed by the lint is exactly `apis.<engine>.tool_calls[].name + aliases` plus `tool_compatibility.engine_exclusive_tools` — both already produced by the agent. Once the schema becomes consumable, the lint becomes a thin wrapper:
```text
For a plugin with no `engines` restriction in aipm.toml, scan its agent/skill/hook
frontmatter for tool names; if any name (or alias) appears in
engine_exclusive_tools[t].supported_by where supported_by ⊊ all_engines,
emit a warning naming which engines it's restricted to.
```

### 6. External Rust ecosystem options (with versions, May 2026)

#### a. `typify` — JSON Schema → Rust types
- [crates.io](https://crates.io/crates/typify), [github.com/oxidecomputer/typify](https://github.com/oxidecomputer/typify)
- Three modes: macro (`typify::import_types!("schema.json")`), `build.rs` programmatic API, or `cargo typify` CLI emitting checked-in `.rs`.
- **Consumes JSON Schema, not raw JSON.** A meta-schema is required.
- Caveats: `anyOf` imprecisely modeled, numeric `min/max` not enforced in types, hard-wired `chrono`/`uuid` for some formats. Default `with_struct_builder(true)` emits internal `.unwrap()` — would clash with the workspace's `unwrap_in_result` lint; prefer plain `Deserialize` round-trip.
- Build-script example: [oxidecomputer/typify/example-build/build.rs](https://github.com/oxidecomputer/typify/blob/main/example-build/build.rs).

#### b. `schemars` — Rust → JSON Schema (inverse)
- [crates.io](https://crates.io/crates/schemars) v1.2.1, [github.com/GREsau/schemars](https://github.com/GREsau/schemars), [docs](https://graham.cool/schemars/).
- Defaults to JSON Schema 2020-12, respects `#[serde(...)]`. Inverse direction: source-of-truth lives in Rust types, JSON validates against them — opposite of what we want unless used **only to bootstrap the meta-schema once**, then check it in.

#### c. `serde` + `LazyLock` + `include_str!`
- `std::sync::LazyLock` stable since Rust 1.80; clippy-recommended over `lazy_static!` and `once_cell::Lazy` ([clippy#12895](https://github.com/rust-lang/rust-clippy/issues/12895)).
- Minimum-viable: `static SCHEMA_JSON: &str = include_str!(...)` + `LazyLock::new(|| serde_json::from_str(...).unwrap_or_else(...))`.
- Gotcha: `unwrap`/`panic!` in initializer violates CLAUDE.md lints — must use `OnceLock::get_or_try_init` returning `Result<&'static T, E>` or fail at startup.
- No compile-time validation that JSON parses. Must pair with CI validation.

#### d. `build.rs` codegen → `.rs` in `OUT_DIR`
- Canonical pattern: write `.rs` to `OUT_DIR`, `include!(concat!(env!("OUT_DIR"), "/foo.rs"))` from `lib.rs`. `cargo:rerun-if-changed=...` for the JSON.
- Real-world references: [`chrono-tz`](https://github.com/chronotope/chrono-tz) + [`chrono-tz-build`](https://docs.rs/chrono-tz-build), unicode-rs family, [intellij-rust/code-generation-example](https://github.com/intellij-rust/code-generation-example).
- Pros: zero startup cost, all data in `&'static` rodata, compiler type-checks generated code (drift = compile error). Cons: slower clean builds, harder IDE navigation into `OUT_DIR`.

#### e. `phf` — perfect-hash compile-time maps
- [crates.io/crates/phf](https://crates.io/crates/phf) v0.13.1, [`phf_codegen`](https://docs.rs/phf_codegen) v0.13.1.
- Ideal for `valid-tool-name`: O(1) `phf::Set<&'static str>` of valid tool names per engine.
- Macro form for small all-literal sets; `phf_codegen::Set` from `build.rs` for data-driven sets. Caveat: very long method chains overflow rustc stack — split if tools list exceeds ~500. Alternative: [`quickphf`](https://docs.rs/quickphf).

#### f. `prost-build` / `tonic-build` conventions
- [`tonic-build`](https://docs.rs/tonic-build/), [`prost-build::Config`](https://docs.rs/prost-build/latest/prost_build/struct.Config.html). Reference patterns: always `cargo:rerun-if-changed=…`, always emit to `OUT_DIR`, always run output through [`prettyplease`](https://crates.io/crates/prettyplease) for readable line numbers and `cargo expand` parity.

#### g. `progenitor` (uses `typify` internally)
- [github.com/oxidecomputer/progenitor](https://github.com/oxidecomputer/progenitor) — Oxide's OpenAPI → Rust client generator. Same maintainers as `typify`. The build.rs template is:
  ```rust
  let spec = serde_json::from_reader(File::open(src)?)?;
  let mut g = progenitor::Generator::default();
  let tokens = g.generate_tokens(&spec)?;
  let ast = syn::parse2(tokens)?;
  std::fs::write(out, prettyplease::unparse(&ast))?;
  ```
- Lesson: the Oxide house style is "build.rs + typify + prettyplease + `include!`" — *not* macro form — explicitly because output is then visible, reviewable, IDE-navigable, and step-debuggable.

#### h. `jsonschema` validation
- [crates.io/crates/jsonschema](https://crates.io/crates/jsonschema) v0.46.3 (April 2026). [`jsonschema-cli`](https://lib.rs/crates/jsonschema-cli).
- 75–645× faster than `valico`, 2–52× faster than `boon` per their benchmarks.
- Use as a CI gate: validate `engine-api-schema.json` against the meta-schema before allowing the weekly PR to merge.

#### i. The hybrid pattern (online research's recommendation)
1. **Meta-schema**: `crates/libaipm/schema/engine-api.schema.json` — hand-authored JSON Schema describing `engine-api-schema.json`'s shape. Bootstrap via `schemars::schema_for!` on draft Rust types, then check in.
2. **`build.rs`** in `libaipm`: runs `typify` over the meta-schema → `engine_api_types.rs` in `OUT_DIR`; same script feeds `phf_codegen` from the *data* file → `valid_tools.rs`.
3. **CI integration test**: uses `jsonschema` to validate the data file against the meta-schema. Weekly agentic PR fails if the regen drifts.
4. **Embed**: `include_str!` the JSON; parse once via `LazyLock<EngineApiSchema>` where the type is the typify-generated one.
5. **Lints**: consult the `phf::Set` directly — branchless hot path.

This combo has no canonical write-up but is implicit in [`oxide.rs`](https://github.com/oxidecomputer/oxide.rs).

### 7. Historical research context

**Primary** (~12 docs):
- [`research/engine-api-schema.json`](../engine-api-schema.json) and [`engine-api-changelog.md`](../engine-api-changelog.md) — the artifact and its diff log.
- [`research/tickets/2026-05-01-510-aipm-toml-engines.md`](../tickets/2026-05-01-510-aipm-toml-engines.md) — `[engines]` block in `aipm.toml`; direct schema consumer.
- [`research/docs/2026-05-01-engine-tool-references.md`](2026-05-01-engine-tool-references.md) — catalogs engine tool references; core input for `valid-tool-name`.
- [`research/docs/2026-05-02-engine-instructions-md-pattern-removal.md`](2026-05-02-engine-instructions-md-pattern-removal.md) — withdrew the `<engine>-instructions.md` classifier branch (commit `cdd2f32`). Aligns with verified facts in `engine-api-schema.json`.
- [`research/docs/2026-03-28-copilot-cli-source-code-analysis.md`](2026-03-28-copilot-cli-source-code-analysis.md), [`2026-03-31-cli-binary-frontmatter-hook-analysis.md`](2026-03-31-cli-binary-frontmatter-hook-analysis.md) — manual predecessors of the current automated reverse-binary-analysis methodology.
- Claude/Copilot defaults specs: [`2026-03-16-claude-code-defaults.md`](2026-03-16-claude-code-defaults.md), [`2026-03-16-copilot-agent-discovery.md`](2026-03-16-copilot-agent-discovery.md), [`2026-03-24-claude-code-agents-format.md`](2026-03-24-claude-code-agents-format.md), [`2026-03-24-claude-code-hooks-settings-styles.md`](2026-03-24-claude-code-hooks-settings-styles.md), [`2026-03-24-claude-code-mcp-lsp-config.md`](2026-03-24-claude-code-mcp-lsp-config.md).
- [`2026-03-28-copilot-cli-migrate-adapter.md`](2026-03-28-copilot-cli-migrate-adapter.md), [`2026-05-01-github-copilot-skills-migrate-lint-silent-failure.md`](2026-05-01-github-copilot-skills-migrate-lint-silent-failure.md).

**Secondary** (~25 docs): manifest format comparison ([`2026-03-09-manifest-format-comparison.md`](2026-03-09-manifest-format-comparison.md)), `aipm.toml` generation ([`2026-03-24-aipm-toml-generation-in-init-and-migrate.md`](2026-03-24-aipm-toml-generation-in-init-and-migrate.md)), aipm-toml editor experience ([`2026-04-19-aipm-toml-editor-experience.md`](2026-04-19-aipm-toml-editor-experience.md)), lint architecture ([`2026-03-31-110-aipm-lint-architecture-research.md`](2026-03-31-110-aipm-lint-architecture-research.md)), lint configuration ([`2026-04-02-aipm-lint-configuration-research.md`](2026-04-02-aipm-lint-configuration-research.md)), lint rules 287/288/289/290 ([`2026-04-07-lint-rules-287-288-289-290.md`](2026-04-07-lint-rules-287-288-289-290.md)), migrate command series, `make`/`pack` foundational APIs.

**Tangential** (~18 docs): NPM/Cargo/pnpm core principles, distribution and packaging, dry-rust audit, coverage and tooling docs, scattered bug-fix notes.

**Notable absence**: no existing research doc discusses **build-script codegen of Rust types from `engine-api-schema.json`**. This is green-field.

## Code References

- `crates/libaipm/src/engine.rs:13-21` — primary `Engine` enum (`Claude`, `Copilot`)
- `crates/libaipm/src/engine.rs:25-53` — `marker_paths`, `marketplace_manifest_path`, `name`, `all_names` `const fn` lookups
- `crates/libaipm/src/engine.rs:155-157` — plugin-marker validation
- `crates/libaipm/src/discovery/types.rs:38-46` — second `Engine` enum (`Claude`, `Copilot`, `Ai`)
- `crates/libaipm/src/discovery/source.rs:37-54` — `infer_engine_root` typed walker
- `crates/libaipm/src/discovery/instruction.rs:32-39` — `INSTRUCTION_FILENAMES` table (only centralized convention-file list)
- `crates/libaipm/src/discovery/layout.rs:52` — Copilot-only layout accommodation
- `crates/libaipm/src/discovery_legacy.rs:48-71` — legacy pattern walker
- `crates/libaipm/src/lint/rules/known_events.rs:10-69` — `CLAUDE_EVENTS`, `COPILOT_EVENTS`, `COPILOT_LEGACY_MAP` (hand-maintained from binary analysis docs)
- `crates/libaipm/src/lint/rules/known_events.rs:75-84` — `is_valid_event` dispatch
- `crates/libaipm/src/lint/rules/scan.rs:33-44` — string-typed engine detector (parallel to `discovery/source.rs`)
- `crates/libaipm/src/lint/rules/skill_name_too_long.rs:13` — `MAX_SKILL_NAME_LENGTH = 64`
- `crates/libaipm/src/lint/rules/skill_desc_too_long.rs:13` — `MAX_DESCRIPTION_LENGTH = 1024`
- `crates/libaipm/src/lint/rules/skill_name_invalid.rs:13-24` — manual byte regex for skill names
- `crates/libaipm/src/lint/rules/instructions_oversized.rs:20-22` — instruction file limits
- `crates/libaipm/src/lint/config.rs:64-69` — only existing `OnceLock` use
- `crates/libaipm/src/manifest/types.rs:11, :46, :66` — `Manifest`, `Package`, `engines: Option<Vec<String>>`
- `crates/libaipm/src/manifest/validate.rs:55, :92-103, :118, :179` — name regex / version validators
- `crates/libaipm/src/make/engine_features.rs:8-23, :82-87, :96-101` — `Feature` enum + `CLAUDE_FEATURES` / `COPILOT_FEATURES` arrays + string-keyed dispatch
- `crates/libaipm/src/migrate/adapters/{agent,hook,skill}.rs` — per-engine adapter `applies_to` predicates
- `crates/libaipm/src/migrate/{agent,command,hook,mcp,skill,output_style}_detector.rs` + `copilot_*_detector.rs` — 13 detector files (one per (engine, feature) pair)
- `crates/aipm/src/main.rs:1074-1077` — CLI engine string validation
- `.github/workflows/reverse-binary-analysis.md:74-82, :128-164, :182-206, :212-254` — schema shape (in prose) + extraction prompts + tool-compatibility analysis
- `.github/workflows/reverse-binary-analysis.lock.yml:52-54, :423, :714` — cron, safe-output config, model id
- `research/engine-api-schema.json:1-455` — the artifact

## Architecture Documentation

**Current pattern** (pre-schema-driven):
- Each subsystem (`discovery`, `migrate`, `lint`, `make`, `manifest`, `workspace_init`) **independently re-encodes** the slice of engine knowledge it needs, as inline `match` arms, `&[&str]` constants, and per-engine detector files.
- New engine support requires editing **N files** (the `Engine` enum, both copies; every `match self` arm; per-engine detectors; `make/engine_features.rs`; CLI engine validation; lint rules; etc.).
- The agentic workflow output is **read-only documentation**, not a build artifact.

**What "source of truth" requires**:
1. Schema must have a **frozen, validated shape** — meta-schema check in CI.
2. The two `Engine` enums must converge or stand in a clear refinement relationship (one is a subset of the other).
3. Free-text fields (`detection_heuristics`, `discovery_algorithm`, `rules`) need either machine-typed shadow fields or explicit `notes`-only quarantine.
4. Codegen mechanism chosen and applied consistently (precedent for the workspace).
5. Generated artifacts replace the existing hand-maintained tables (`known_events.rs`, `instruction.rs`, `engine_features.rs`, scattered string literals).
6. The agent's PR labels must trigger CI that runs schema validation + Rust build, so that any schema-vs-consumer mismatch surfaces as a failed PR.

**Conventions copy-pastable from external ecosystem**:
- Always `cargo:rerun-if-changed=research/engine-api-schema.json`.
- Emit generated `.rs` to `OUT_DIR` (visible in target/, not checked in).
- Run output through `prettyplease` for readable diagnostics and `cargo expand` parity.
- Validate JSON in CI before any consumer runs.
- Force-evaluate `LazyLock` in `main()` if fail-fast is wanted.
- Avoid `unwrap`/`panic!` in initializers (CLAUDE.md lints).

## Historical Context (from research/)

- [`research/docs/2026-05-01-engine-tool-references.md`](2026-05-01-engine-tool-references.md) — already catalogs the engines × tools matrix that this schema now produces automatically. The two should be reconciled; the older doc may be superseded.
- [`research/tickets/2026-05-01-510-aipm-toml-engines.md`](../tickets/2026-05-01-510-aipm-toml-engines.md) — designs `[engines]` block in `aipm.toml` (engine name + version pin). Direct downstream consumer of `versions` and `apis.<engine>` from the schema.
- [`research/docs/2026-03-31-cli-binary-frontmatter-hook-analysis.md`](2026-03-31-cli-binary-frontmatter-hook-analysis.md) — original manual binary analysis that hand-populated `known_events.rs`. The reverse-binary-analysis workflow automates exactly this — but the constants in `known_events.rs` are still hand-edited.
- [`research/docs/2026-05-02-engine-instructions-md-pattern-removal.md`](2026-05-02-engine-instructions-md-pattern-removal.md) (commit `cdd2f32`) — recently withdrew an unverified classification rule. The lesson — *don't ship classification logic the schema doesn't justify* — directly motivates schema-as-source-of-truth.
- [`research/docs/2026-04-19-aipm-toml-editor-experience.md`](2026-04-19-aipm-toml-editor-experience.md) — VS Code editor experience for `aipm.toml`. If a JSON Schema (or `aipm.toml.schema.json`-style) is generated from `engine-api-schema.json`, it powers IDE completion for engine names and versions.
- [`research/docs/2026-04-12-dry-rust-architecture-audit.md`](2026-04-12-dry-rust-architecture-audit.md) — architecture audit explicitly flags duplication; centralizing engine data via codegen aligns with its findings.

## Related Research

- [`research/docs/2026-05-01-engine-tool-references.md`](2026-05-01-engine-tool-references.md)
- [`research/tickets/2026-05-01-510-aipm-toml-engines.md`](../tickets/2026-05-01-510-aipm-toml-engines.md)
- [`research/docs/2026-05-02-engine-instructions-md-pattern-removal.md`](2026-05-02-engine-instructions-md-pattern-removal.md)
- [`research/docs/2026-03-31-cli-binary-frontmatter-hook-analysis.md`](2026-03-31-cli-binary-frontmatter-hook-analysis.md)
- [`research/docs/2026-04-12-dry-rust-architecture-audit.md`](2026-04-12-dry-rust-architecture-audit.md)
- [`research/docs/2026-04-19-aipm-toml-editor-experience.md`](2026-04-19-aipm-toml-editor-experience.md)
- [`research/engine-api-changelog.md`](../engine-api-changelog.md)

## Open Questions

1. **Schema shape ownership** — should the meta-schema (`engine-api.schema.json`) be hand-authored once and committed, generated from a Rust source-of-truth via `schemars` (inverting the relationship), or co-evolved by the agent itself? The agent today is told the shape only in prose — a checked-in meta-schema would let the agent self-validate before opening a PR.
2. **`Engine` enum reconciliation** — does `Engine::Ai` from `discovery/types.rs` belong in the schema (as a marketplace-host pseudo-engine), or is it a discovery-internal concept that should stay separate? The schema currently treats only `claude` and `copilot-cli` as engines; `Ai` is invisible to it.
3. **Free-text field strategy** — `detection_heuristics`, `discovery_algorithm`, `rules` are prose `string[]`. Promote to structured (`{kind, target}` records), keep as `notes` for human review only, or split into `data` + `notes` partitions?
4. **Codegen scope** — should generated tables fully replace `known_events.rs`, `instruction.rs`, `engine_features.rs`, and the scattered path literals; or coexist with hand-written code that consults them? Full replacement is cleaner but more invasive.
5. **CI gating of the agent's PR** — the agent's `safe-outputs` config protects `.github/`, `.agents/`, etc. but does NOT protect `crates/`. Should the workflow be extended (or a separate CI check added) so that a regen-PR is auto-validated against the meta-schema, the Rust workspace builds with `cargo build --workspace -- -D warnings`, and the schema-driven tests pass — *before* the PR can merge?
6. **Where does the meta-schema and the codegen live?** A new `crates/libaipm-engine-spec/` crate (data-only, with its own `build.rs`) keeps codegen blast-radius small; alternatively `libaipm` grows a `build.rs`. The former isolates lint-policy carve-outs (the typify output may need lint relaxations that should not bleed into `libaipm`).
7. **`unwrap_in_result` lint vs typify default builders** — typify's `with_struct_builder(true)` emits internal `.unwrap()` calls. Either disable struct-builder generation, or restrict typify output to a child crate where the lint is explicitly relaxed for `OUT_DIR` files only (and only if such a relaxation is achievable — CLAUDE.md forbids `#[allow]` attributes globally).
8. **Prose suggestions field** — `suggestions.<engine>.{adaptor_fixes, test_cases, behaviour_variants}` is the agent's natural-language advice. Should this be machine-actionable (auto-open issues for each item), human-only (rendered into the PR body), or both?
9. **Versioning the meta-schema** — when the meta-schema itself changes, how do older committed `engine-api-schema.json` snapshots interact? A `$schema` URI per regen plus a `meta_schema_version` field would let the codegen skip incompatible historical snapshots.
10. **Replacing `known_events.rs` is not a 1:1 translation** — that file's `COPILOT_LEGACY_MAP` carries information the schema does not currently emit (legacy hook event aliases). The schema would need to grow a `legacy_map`/`deprecated_aliases` field on `tool_calls` entries before it can subsume the file.
