---
date: 2026-05-05 15:55:00 UTC
researcher: Sean Larkin
git_commit: ad39977
branch: main
repository: aipm
topic: "`aipm init` should ask for engine in wizard prompts, and then follow the correct init folder (#724)"
tags: [research, tickets, issue-724, init, wizard, engines, scaffolding, multi-engine]
status: complete
last_updated: 2026-05-05
last_updated_by: Sean Larkin
---

# Research

## Research Question

Source: [GitHub issue #724](https://github.com/TheLarkInn/aipm/issues/724) —
*"[init] `aipm init` should ask for engine in wizard prompts, and then follow
the correct init folder"*. The body reads:

> It feels bad to have a .claude folder generated for `aipm init` for a team
> that never uses `claude`. Just confusing. Lets fix this.

This research documents the **current** behavior of `aipm init` end-to-end so a
spec author can plan the engine-aware wizard fix with full awareness of every
touchpoint: CLI entry, wizard prompts (TTY + non-TTY), the `Options` handoff,
the `ToolAdaptor` registration, the unconditional `.claude/` creation, the
`aipm.toml` engine schema, the engine catalog, and the BDD/Rust tests that lock
the existing behavior.

Companion deep-dives produced as part of this research:

- [`research/docs/2026-05-05-init-cli-entry-point.md`](../docs/2026-05-05-init-cli-entry-point.md)
- [`research/docs/2026-05-05-wizard-prompt-flow.md`](../docs/2026-05-05-wizard-prompt-flow.md)
- [`research/docs/2026-05-05-engine-catalog.md`](../docs/2026-05-05-engine-catalog.md)
- [`research/docs/2026-05-05-init-scaffolding-trace.md`](../docs/2026-05-05-init-scaffolding-trace.md)
- [`research/docs/2026-05-05-aipm-toml-engine-schema.md`](../docs/2026-05-05-aipm-toml-engine-schema.md)

## Summary

1. **`aipm init` never asks about engine and unconditionally writes `.claude/`
   whenever a marketplace is being scaffolded.** The wizard asks three
   questions — setup mode (Marketplace / Workspace / Both), marketplace name,
   and starter-plugin yes/no — and exits with a 4-tuple
   (`workspace, marketplace, no_starter, marketplace_name`). There is no
   `--engine` flag on `Init`. There is no engine field on
   `libaipm::workspace_init::Options`. Source:
   [`crates/aipm/src/wizard.rs:23-73`](../../crates/aipm/src/wizard.rs),
   [`crates/aipm/src/main.rs:35-64`](../../crates/aipm/src/main.rs#L35-L64),
   [`crates/libaipm/src/workspace_init/mod.rs:51-65`](../../crates/libaipm/src/workspace_init/mod.rs#L51-L65).
2. **Engine selection lives entirely in a single hardcoded factory function**
   ([`crates/libaipm/src/workspace_init/adaptors/mod.rs:13-15`](../../crates/libaipm/src/workspace_init/adaptors/mod.rs#L13-L15)):

   ```rust
   pub fn defaults() -> Vec<Box<dyn ToolAdaptor>> {
       vec![Box::new(claude::Adaptor)]
   }
   ```

   The doc comment on the same lines reads *"Currently only includes Claude
   Code. Future adaptors (Copilot CLI, OpenCode, etc.) are added here."*
3. **The unconditional `.claude/` mkdir is at
   [`crates/libaipm/src/workspace_init/adaptors/claude.rs:26-29`](../../crates/libaipm/src/workspace_init/adaptors/claude.rs#L26-L29)**:

   ```rust
   let settings_dir = dir.join(".claude");
   let settings_path = settings_dir.join("settings.json");

   fs.create_dir_all(&settings_dir)?;
   ```

   `fs.create_dir_all` runs **before** the change-detection logic that returns
   `Ok(false)` when no settings change is needed, so the directory is created
   even when no `.claude/settings.json` is written. The literal string
   `".claude"` is hardcoded; it does not use
   `libaipm_engine_spec::paths::CLAUDE_DOT`.
4. **The `Package.engines` field already exists, parses, and round-trips** —
   but the init code path never reads or writes it. The field is declared as
   `Option<EngineSet>` (a `bitflags` u32 from the `libaipm-engine-spec` crate)
   at
   [`crates/libaipm/src/manifest/types.rs:65-83`](../../crates/libaipm/src/manifest/types.rs#L65-L83)
   with three semantic states (omitted = all engines; `[]` = all engines;
   non-empty list = bitset of recognized names). The companion JSON schema at
   `schemas/aipm.toml.schema.json` does **not** include the `engines` property.
5. **Only two engines are currently recognized** by `libaipm-engine-spec`:
   `claude` (root: `.claude/`) and `copilot-cli` (root: `.github/`). The
   filesystem-root mapping is hardcoded inside
   [`crates/libaipm-engine-spec/build.rs:349-368`](../../crates/libaipm-engine-spec/build.rs#L349-L368)
   (`CLAUDE_DOT = ".claude"`, `GITHUB_DOT = ".github"`, `AI_DOT = ".ai"`).
   `MarketplaceHost::Ai` (root: `.ai/`) is not an engine — it's a separate
   marketplace concept.
6. **The five `aipm.toml` writers all pass `engines: None`**, except one — the
   synthetic starter plugin at
   [`crates/libaipm/src/workspace_init/mod.rs:282`](../../crates/libaipm/src/workspace_init/mod.rs#L282)
   which hardcodes `let starter_engines: &[&str] = &["claude"];`. None of the
   writers consult the user's selection because the user is never asked.
7. **A symmetric, working pattern already exists in `aipm make plugin`.** Its
   wizard at
   [`crates/aipm/src/wizard.rs:297, 319-325`](../../crates/aipm/src/wizard.rs#L297)
   uses `ENGINE_OPTIONS = &["Claude Code", "Copilot CLI", "Both"]` and a
   "Target engine" prompt; its CLI flag `--engine claude/copilot/both` is at
   [`crates/aipm/src/main.rs:270-272`](../../crates/aipm/src/main.rs#L270-L272).
   This is the precedent that #724 mirrors for `Init`.
8. **The behavior in #724 is locked by tests** — both the BDD scenarios at
   [`tests/features/manifest/workspace-init.feature:81-85, 173-186`](../../tests/features/manifest/workspace-init.feature)
   and the Rust integration test
   [`crates/aipm/tests/init_e2e.rs:22-50`](../../crates/aipm/tests/init_e2e.rs#L22-L50)
   assert `.claude/settings.json` is always created. The library-level test
   `init_with_no_adaptors` at
   [`crates/libaipm/src/workspace_init/mod.rs:585-594`](../../crates/libaipm/src/workspace_init/mod.rs#L585-L594)
   demonstrates that `.claude/` is suppressed only when the `adaptors` slice
   passed into the library API is empty — but the CLI never does that.

## Detailed Findings

### 1. CLI entry & call graph

`aipm init` clap struct ([`crates/aipm/src/main.rs:35-64`](../../crates/aipm/src/main.rs#L35-L64)).
Six flags, no `--engine`:

| Flag | Default | Purpose |
|---|---|---|
| `-y`, `--yes` | `false` | skip prompts, use defaults |
| `--workspace` | `false` | generate workspace `aipm.toml` |
| `--marketplace` | `false` | generate `.ai/` marketplace + tool settings |
| `--no-starter` | `false` | skip starter plugin |
| `--manifest` | `false` | generate plugin `aipm.toml` |
| `--name <NAME>` | `None` | custom marketplace name |
| `dir` positional | `"."` | target directory |

Dispatch flow:

1. `main() → run() → Commands::Init` ([`main.rs:1201`](../../crates/aipm/src/main.rs#L1201))
2. `cmd_init` ([`main.rs:398-445`](../../crates/aipm/src/main.rs#L398-L445)) computes
   `let interactive = !flags.yes && std::io::stdin().is_terminal();` (line 405)
3. `wizard_tty::resolve(...)` returns
   `(do_workspace, do_marketplace, do_no_starter, marketplace_name)` —
   [`crates/aipm/src/wizard_tty.rs:37-51`](../../crates/aipm/src/wizard_tty.rs#L37-L51)
4. `let adaptors = libaipm::workspace_init::adaptors::defaults();`
   ([`main.rs:413`](../../crates/aipm/src/main.rs#L413)) — **always Claude**
5. Constructs `libaipm::workspace_init::Options` with the 4-tuple plus
   `manifest` flag — no engine field
   ([`main.rs:414-421`](../../crates/aipm/src/main.rs#L414-L421))
6. Calls `libaipm::workspace_init::init(&opts, &adaptors, &libaipm::fs::Real)`
   ([`main.rs:423`](../../crates/aipm/src/main.rs#L423))

### 2. Wizard prompt flow

Three prompts, captured at
[`crates/aipm/src/wizard.rs:23-73`](../../crates/aipm/src/wizard.rs#L23-L73):

1. **"What would you like to set up?"** (Select) — Marketplace only / Workspace
   only / Both. Default index `0`. Shown only when neither `--workspace` nor
   `--marketplace` is set.
2. **"Marketplace name:"** (Text) — placeholder `"local-repo-plugins"`,
   validated by `manifest::validate::check_name`. Shown only if marketplace is
   in scope and `--name` is not set.
3. **"Include starter plugin?"** (Confirm) — default `true`. Shown only if
   marketplace is in scope and `--no-starter` is not set.

`ENGINE_OPTIONS = &["Claude Code", "Copilot CLI", "Both"]` exists at
[`crates/aipm/src/wizard.rs:297`](../../crates/aipm/src/wizard.rs#L297) but is
consumed only by the `aipm make plugin` wizard, not by `aipm init`. Snapshot
files under
[`crates/aipm/src/snapshots/`](../../crates/aipm/src/snapshots/) (29+ files)
lock the current prompt sequence.

Non-interactive defaults at
[`crates/aipm/src/wizard.rs:140-150`](../../crates/aipm/src/wizard.rs#L140-L150):
no flags → `(workspace=false, marketplace=true, no_starter=false,
marketplace_name="local-repo-plugins")`. Marketplace ON by default → adaptor
loop runs → `.claude/` created.

### 3. Engine catalog (`libaipm-engine-spec`)

Two engines today, generated by `build.rs` from
[`crates/libaipm-engine-spec/data/engine-api-schema.json:5-16`](../../crates/libaipm-engine-spec/data/engine-api-schema.json#L5-L16):

| Engine variant | Schema name | Root dir | NPM package |
|---|---|---|---|
| `Engine::Claude` | `claude` | `.claude` | `@anthropic-ai/claude-code` |
| `Engine::CopilotCli` | `copilot-cli` | `.github` | `@github/copilot` |
| `MarketplaceHost::Ai` (not an engine) | — | `.ai` | — |

Generated artefacts in `OUT_DIR/engine_data.rs`:

- `Engine` enum (PascalCase variants from `to_pascal_case`)
- `EngineSet` (u32 bitflags, 1 bit per engine; `CLAUDE`, `COPILOT_CLI`, `ALL`)
- `Engine::ALL`, `Engine::name`, `Engine::from_name`, `Engine::as_set`
- `ENGINES: &[(Engine, EngineSpec)]`
- `TOOL_COMPATIBILITY`, `HOOK_EVENTS_BY_ENGINE`, `FEATURES_BY_ENGINE`
- `pub mod paths { CLAUDE_DOT, GITHUB_DOT, AI_DOT }` (hardcoded in
  [`build.rs:349-368`](../../crates/libaipm-engine-spec/build.rs#L349-L368))
- `pub mod constraints`
- `valid_tools.rs` with a `phf::Set`

`crates/aipm/` does not depend on `libaipm-engine-spec` directly — it pulls the
re-exports from `libaipm` at
[`crates/libaipm/src/lib.rs:44`](../../crates/libaipm/src/lib.rs#L44):

```rust
pub use libaipm_engine_spec::{constraints, paths, Engine, EngineSet, MarketplaceHost};
```

`crates/libaipm/` has 25+ import sites (engine.rs, discovery/, lint/rules/,
manifest/, migrate/, make/engine_features.rs).

### 4. Scaffolding pipeline (where `.claude/` lands)

Library entry point: `libaipm::workspace_init::init` at
[`crates/libaipm/src/workspace_init/mod.rs:96-120`](../../crates/libaipm/src/workspace_init/mod.rs#L96-L120):

```rust
if opts.workspace {
    init_workspace(opts.dir, fs)?;
}
if opts.marketplace {
    scaffold_marketplace(opts.dir, opts.no_starter, opts.manifest, opts.marketplace_name, fs)?;
    for adaptor in adaptors {
        if adaptor.apply(opts.dir, opts.no_starter, opts.marketplace_name, fs)? { … }
    }
}
```

Adaptor loop is gated only on `opts.marketplace`. Inside
`claude::Adaptor::apply`
([`adaptors/claude.rs:19-69`](../../crates/libaipm/src/workspace_init/adaptors/claude.rs#L19-L69)),
the directory is created before any change-detection. Confirmed by the test
[`init_no_starter_still_configures_tools`](../../crates/libaipm/src/workspace_init/mod.rs#L868-L905):

```rust
assert!(tmp.join(".claude/settings.json").exists());
assert!(!tmp.join(".ai/starter-aipm-plugin").exists());
```

`--no-starter` does **not** suppress `.claude/`.

Conditional matrix (default flags = marketplace-only):

| Path | Created? | Gate |
|---|---|---|
| `<dir>/aipm.toml` | only with `--workspace` | `if opts.workspace` |
| `<dir>/.ai/` and children | yes by default | `if opts.marketplace` |
| `.ai/starter-aipm-plugin/...` | yes by default | `!no_starter` early-return |
| `.ai/starter-aipm-plugin/aipm.toml` | only with `--manifest` | `if manifest` |
| **`<dir>/.claude/`** | **yes by default — UNCONDITIONAL** | `if opts.marketplace` only |
| `<dir>/.claude/settings.json` | yes by default | conditional inside adaptor |

Templates are inline string literals (no `include_str!`/`include_bytes!` in
`workspace_init/`). The starter manifest writer at
[`workspace_init/mod.rs:276-299`](../../crates/libaipm/src/workspace_init/mod.rs#L276-L299)
hardcodes `engines = ["claude"]`.

### 5. `aipm.toml` engine schema

`Package.engines: Option<EngineSet>` at
[`crates/libaipm/src/manifest/types.rs:65-83`](../../crates/libaipm/src/manifest/types.rs#L65-L83):

```rust
#[serde(default, deserialize_with = "engine_set_serde::deserialize")]
pub engines: Option<EngineSet>,
```

Custom deserializer at
[`types.rs:323-357`](../../crates/libaipm/src/manifest/types.rs#L323-L357)
enforces three semantic states (omitted/empty/non-empty) and rejects
all-unknown lists with a custom error. **No corresponding `Serialize`
implementation** — TOML emission goes through `toml_edit` in
[`crates/libaipm/src/manifest/builder.rs`](../../crates/libaipm/src/manifest/builder.rs).
`build_plugin_manifest` accepts `engines: Option<&[&str]>` at
[`builder.rs:11-24`](../../crates/libaipm/src/manifest/builder.rs#L11-L24)
and inserts `engines = […]` only if `Some(non-empty)` at
[`builder.rs:72-80`](../../crates/libaipm/src/manifest/builder.rs#L72-L80).

Five writer call sites:

| # | Site | Engines passed |
|---|---|---|
| 1 | [`init.rs:158-179`](../../crates/libaipm/src/init.rs#L158-L179) — `aipm pack init` | `None` |
| 2 | [`workspace_init/mod.rs:144-168`](../../crates/libaipm/src/workspace_init/mod.rs#L144-L168) — `generate_workspace_manifest` | n/a (workspace path has no engines field) |
| 3 | [`workspace_init/mod.rs:276-299`](../../crates/libaipm/src/workspace_init/mod.rs#L276-L299) — `generate_starter_manifest` | `Some(&["claude"])` (hardcoded) |
| 4 | [`migrate/emitter.rs:914-922`](../../crates/libaipm/src/migrate/emitter.rs#L914-L922) | `None` |
| 5 | [`migrate/emitter.rs:1136-1143`](../../crates/libaipm/src/migrate/emitter.rs#L1136-L1143) | `None` |

There is no validation pass on `engines` after deserialization — the only
filter is the deserialize-time check. `manifest::Error` has no
`InvalidEngine` variant.

A second, parallel deserializer (`MinimalManifest`/`MinimalPackage`) lives in
[`crates/libaipm/src/engine.rs:101-148`](../../crates/libaipm/src/engine.rs#L101-L148)
for plugin-acquisition validation; it stays as `Vec<String>` instead of
`EngineSet`.

The companion JSON schema
[`schemas/aipm.toml.schema.json`](../../schemas/aipm.toml.schema.json) does
**not** declare an `engines` property — only the `[workspace.lints]` block is
constrained.

### 6. Tests that lock current behavior

**BDD** —
[`tests/features/manifest/workspace-init.feature`](../../tests/features/manifest/workspace-init.feature):

```gherkin
# lines 81-85
Scenario: No-starter flag still configures tool settings
  Given an empty directory "my-project"
  When the user runs "aipm init --no-starter" in "my-project"
  Then a file ".claude/settings.json" exists in "my-project"
  And there is no directory ".ai/starter-aipm-plugin" in "my-project"
```

```gherkin
# lines 173-186 — Rule: Tool settings integration
Scenario: Claude Code settings point to .ai/ as local marketplace
  …
  Then a file ".claude/settings.json" exists in "my-project"
```

Step glue at [`crates/libaipm/tests/bdd.rs:594-650`](../../crates/libaipm/tests/bdd.rs#L594-L650).

**Rust integration** —
[`crates/aipm/tests/init_e2e.rs`](../../crates/aipm/tests/init_e2e.rs):

- `init_default_creates_marketplace_only` (line 22, asserts at line 38)
- `init_claude_settings_generated` (line 135)
- `init_settings_json_marketplace_name_and_enabled_plugins` (line 205)
- `scaffold_script_enables_in_settings_json` (line 298)
- `scaffold_script_multiple_plugins_no_duplicates` (line 342)

**Library unit** — `crates/libaipm/src/workspace_init/`:

- [`mod.rs:868-905`](../../crates/libaipm/src/workspace_init/mod.rs#L868-L905) —
  `init_no_starter_still_configures_tools`
- [`mod.rs:907-936`](../../crates/libaipm/src/workspace_init/mod.rs#L907-L936) —
  `init_marketplace_with_preconfigured_claude_settings`
- [`mod.rs:585-594`](../../crates/libaipm/src/workspace_init/mod.rs#L585-L594) —
  `init_with_no_adaptors` (the only test demonstrating `.claude/` suppression,
  via empty adaptor slice — a path the CLI never takes)
- [`adaptors/claude.rs:90-310`](../../crates/libaipm/src/workspace_init/adaptors/claude.rs#L90-L310) —
  six tests for the Claude adaptor's create/merge/skip paths

**Snapshot** —
[`crates/libaipm/src/workspace_init/snapshots/libaipm__workspace_init__tests__scaffold_script_snapshot.snap`](../../crates/libaipm/src/workspace_init/snapshots/libaipm__workspace_init__tests__scaffold_script_snapshot.snap)
locks the bash content from `generate_scaffold_script()`, including the
`--engine "${2:-claude}"` default.

### 7. Existing precedent: `aipm make plugin` engine prompt

The shape #724 mirrors already exists for `aipm make plugin`:

- `ENGINE_OPTIONS = &["Claude Code", "Copilot CLI", "Both"]` at
  [`crates/aipm/src/wizard.rs:297`](../../crates/aipm/src/wizard.rs#L297)
- "Target engine" prompt at
  [`crates/aipm/src/wizard.rs:319-325`](../../crates/aipm/src/wizard.rs#L319-L325)
- TTY bridge at
  [`crates/aipm/src/wizard_tty.rs:113-119`](../../crates/aipm/src/wizard_tty.rs#L113-L119)
- CLI flag `--engine claude/copilot/both` on `MakeSubcommand::Plugin` at
  [`crates/aipm/src/main.rs:270-272`](../../crates/aipm/src/main.rs#L270-L272)
- The starter scaffolding script written by `generate_scaffold_script` runs
  `aipm make plugin --name … --engine "${2:-claude}"` at
  [`workspace_init/mod.rs:329-336`](../../crates/libaipm/src/workspace_init/mod.rs#L329-L336)

## Code References

- `crates/aipm/src/main.rs:35-64` — `Init` clap struct (no engine flag)
- `crates/aipm/src/main.rs:398-445` — `cmd_init`
- `crates/aipm/src/main.rs:1201` — top-level `Commands::Init` dispatch
- `crates/aipm/src/wizard.rs:23-73` — `workspace_prompt_steps` (3 prompts)
- `crates/aipm/src/wizard.rs:140-150` — `resolve_defaults`
- `crates/aipm/src/wizard.rs:297` — `ENGINE_OPTIONS`
- `crates/aipm/src/wizard.rs:319-325` — `aipm make plugin` engine prompt
- `crates/aipm/src/wizard_tty.rs:37-51` — `resolve` (interactive switch)
- `crates/libaipm/src/wizard.rs:78-148` — `execute_prompts`
- `crates/libaipm/src/workspace_init/mod.rs:51-65` — `Options` (no engine field)
- `crates/libaipm/src/workspace_init/mod.rs:96-120` — `init` (adaptor loop)
- `crates/libaipm/src/workspace_init/mod.rs:282` — hardcoded
  `starter_engines = &["claude"]`
- `crates/libaipm/src/workspace_init/adaptors/mod.rs:13-15` — hardcoded
  `defaults()`
- `crates/libaipm/src/workspace_init/adaptors/claude.rs:26-29` —
  unconditional `.claude/` mkdir
- `crates/libaipm/src/manifest/types.rs:65-83` — `Package.engines`
- `crates/libaipm/src/manifest/types.rs:323-357` — `engine_set_serde`
- `crates/libaipm/src/manifest/builder.rs:11-24, 72-80` — `engines` emission
- `crates/libaipm/src/lib.rs:44` — engine-spec re-exports
- `crates/libaipm-engine-spec/data/engine-api-schema.json:5-16` — engine list
- `crates/libaipm-engine-spec/build.rs:349-368` — `paths` constants
- `tests/features/manifest/workspace-init.feature:81-85, 173-186` — locked
  `.claude/settings.json` assertions
- `crates/aipm/tests/init_e2e.rs:22-50, 135, 205, 298, 342` — integration tests
- `crates/libaipm/src/workspace_init/mod.rs:585-594` — `init_with_no_adaptors`

## Architecture Documentation

### Two-layer wizard pattern

`aipm` separates wizard *definition* from *execution*. The definition layer
([`crates/aipm/src/wizard.rs`](../../crates/aipm/src/wizard.rs)) returns
`Vec<PromptStep>` as pure data and is fully covered by snapshot tests. The
execution layer
([`crates/aipm/src/wizard_tty.rs`](../../crates/aipm/src/wizard_tty.rs)) calls
`libaipm::wizard::execute_prompts` which renders prompts via `inquire`;
`wizard_tty.rs` is excluded from coverage via
`--ignore-filename-regex` per [CLAUDE.md](../../CLAUDE.md). Engine-aware
extension to `Init` would add prompt steps in the definition layer, then
extend `resolve_workspace_answers` and the `Options` struct.

### `ToolAdaptor` extension seam

The `ToolAdaptor` trait is the existing abstraction for tool-specific
scaffolding. It was introduced by spec
[`2026-03-19-init-tool-adaptor-refactor.md`](../../specs/2026-03-19-init-tool-adaptor-refactor.md)
which deleted prior VS Code and Copilot scaffolding and left only Claude. The
factory at `adaptors::defaults()` is the seam where additional adaptors
register; `init()` iterates whatever the caller passes in. The library API
already supports an empty adaptor slice (verified by
`init_with_no_adaptors`); the CLI does not currently exercise that path.

### Engine schema source-of-truth

Since PR [#771](https://github.com/TheLarkInn/aipm/pull/771) (commit
`14a7f4f`), the canonical engine list is
`crates/libaipm-engine-spec/data/engine-api-schema.json` and the typed `Engine`
enum + `EngineSet` are generated by `build.rs`. `Package.engines`
(`Option<EngineSet>`) consumes this directly via the `engine_set_serde`
deserializer. The init flow pre-dates this schema crate and still references
`".claude"` as a literal string rather than via `paths::CLAUDE_DOT`.

### Marketplace vs engine

`MarketplaceHost::Ai` (root: `.ai/`) is conceptually distinct from engines —
`.ai/` is the tool-agnostic marketplace location, while `.claude/`,
`.github/copilot/`, etc. are per-engine consumption surfaces. `aipm init`
already creates `.ai/` unconditionally (when `--marketplace` is in scope) and
populates `.ai/.claude-plugin/marketplace.json` as the registry index — that
file lives under `.ai/`, not `.claude/`, so its name is misleading. Source:
[`workspace_init/mod.rs:202-214`](../../crates/libaipm/src/workspace_init/mod.rs#L202-L214).

## Historical Context (from research/)

The full prior-art index is in
[`research/docs/2026-05-05-init-cli-entry-point.md`](../docs/2026-05-05-init-cli-entry-point.md)'s
sibling docs (locator-agent output for #724 surfaced ~40 docs). The
load-bearing precedents:

- `research/tickets/2026-05-01-510-aipm-toml-engines.md` — **the master
  three-issue ticket (#510 / #724 / #697).** Predates PR #771 (so it discusses
  `Package.engines: Option<Vec<String>>` rather than `Option<EngineSet>`), but
  documents the per-issue file-touch matrix, the suggested implementation order
  (#510 → #697 → #724), and seven open questions that still apply.
- `specs/2026-03-22-interactive-init-wizard.md` — canonical spec for the
  `inquire` wizard. Section 3.2 explicitly lists "We will NOT modify libaipm"
  as a non-goal — a constraint that #724 will revisit since extending
  `defaults()` and `Options` requires libaipm changes.
- `specs/2026-03-19-init-tool-adaptor-refactor.md` — introduced the
  `ToolAdaptor` trait, deleted VS Code and Copilot scaffolding. Open Question
  3 ("Should the CLI auto-detect adaptors or accept a `--tool` flag?") is the
  explicit seed of #724.
- `specs/2026-04-14-aipm-make-plugin-command.md` — establishes the
  `--engine claude/copilot/both` flag and `ENGINE_OPTIONS` wizard pattern that
  #724 mirrors.
- `specs/2026-03-25-marketplace-name-customization.md` and
  `research/docs/2026-03-25-marketplace-name-customization-in-init.md` —
  precedent for adding a new prompt step + flag, identifies the four layers
  (CLI args, wizard prompts, `Options`, downstream consumers) that any new
  prompt must update.
- `specs/2026-03-24-suppress-plugin-manifest-generation.md` — precedent for
  adding suppression-matrix flags (`--no-starter`, `--manifest`) to `aipm init`.
- `research/docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md` — the
  five `aipm.toml` generation sites that may need to learn an engines field if
  it is plumbed through workspace init.
- `research/docs/2026-05-04-engine-api-schema-source-of-truth.md` and
  `specs/2026-05-04-engine-api-schema-source-of-truth.md` — the spec/research
  pair behind PR #771 (the libaipm-engine-spec crate). Non-Goal NG6 explicitly
  defers the `[engines]` block to #510.
- `research/docs/2026-05-02-engine-instructions-md-pattern-removal.md` /
  `specs/2026-05-02-engine-instructions-md-pattern-removal.md` — withdraws G7
  from the unified-discovery spec; **G7's withdrawal is unrelated to #724** —
  it concerned `<engine>-instructions.md` filename discovery, not the init
  wizard.
- `research/docs/2026-03-16-claude-code-defaults.md`,
  `research/docs/2026-03-28-copilot-cli-migrate-adapter.md`,
  `research/docs/2026-03-28-copilot-cli-source-code-analysis.md`,
  `research/docs/2026-03-31-cli-binary-frontmatter-hook-analysis.md` — folder
  layout ground truth for what each engine's adaptor must scaffold.

## Related Research

- [`research/tickets/2026-05-01-510-aipm-toml-engines.md`](2026-05-01-510-aipm-toml-engines.md)
  — three-issue master ticket (#510 + #724 + #697)
- [`research/docs/2026-05-05-init-cli-entry-point.md`](../docs/2026-05-05-init-cli-entry-point.md)
- [`research/docs/2026-05-05-wizard-prompt-flow.md`](../docs/2026-05-05-wizard-prompt-flow.md)
- [`research/docs/2026-05-05-engine-catalog.md`](../docs/2026-05-05-engine-catalog.md)
- [`research/docs/2026-05-05-init-scaffolding-trace.md`](../docs/2026-05-05-init-scaffolding-trace.md)
- [`research/docs/2026-05-05-aipm-toml-engine-schema.md`](../docs/2026-05-05-aipm-toml-engine-schema.md)

## Open Questions

These are surfaced from the research so the spec author can resolve them
explicitly rather than choose by accident:

1. **Engine-name canonicalization.** `Engine::name` returns `"claude"` and
   `"copilot-cli"` (kebab-case from the schema). The `aipm make plugin` flag
   accepts the values `claude`, `copilot`, and `both`. Existing BDD fixtures
   use both `"copilot"` (legacy, e.g.
   `tests/features/registry/engine-validation.feature:13`) and `"copilot-cli"`
   (canonical). The init wizard's prompt label is "Copilot CLI". Decision
   needed: which form does the wizard write into `[package].engines`?
2. **Workspace-level vs package-level `engines`.** `Package.engines` exists;
   `Workspace` has no equivalent. `aipm init --workspace` produces a workspace
   manifest with no `[package]` section, so the engine selection has nowhere
   to land in the workspace-only path. Should `aipm init` skip the engine
   prompt when only `--workspace` is selected, or should the Workspace struct
   gain an `engines` field?
3. **Adaptor introduction for new engines.** Today only
   `claude::Adaptor` exists; `defaults()` returns one element. A copilot
   adaptor was deleted by spec `2026-03-19` and would need to be implemented
   from scratch — folder layout per
   `research/docs/2026-03-28-copilot-cli-source-code-analysis.md`. Until a
   `copilot::Adaptor` exists, what does `aipm init --engine copilot-cli` do?
   (Skip adaptor loop? Error? Write a placeholder?)
4. **Orphan `.claude/` migration.** Existing projects that ran `aipm init`
   before the fix already have `.claude/` directories. Does the new wizard
   detect a `.claude/settings.json` whose `extraKnownMarketplaces` points at
   `.ai/` and treat that as implicit "Claude is selected"? Or does the user
   need to re-run init?
5. **JSON schema sync.** `schemas/aipm.toml.schema.json` does not declare the
   `engines` property today (only `[workspace.lints]` is constrained). If the
   wizard starts writing it, does the JSON schema get extended too? Owner of
   that file is documented in
   `research/docs/2026-04-19-aipm-toml-editor-experience.md`.
6. **Suppression matrix interaction.** `aipm init --workspace` (alone) skips
   the adaptor loop entirely (no `.claude/`) — the existing behavior for
   workspace-only init is already engine-aware in effect. Does the wizard
   surface this implicitly (don't ask) or explicitly (ask but lock to "none")?
7. **Test churn.** The BDD scenarios at
   `tests/features/manifest/workspace-init.feature:81-85` and `:173-186` and
   the integration tests in `crates/aipm/tests/init_e2e.rs` assert
   `.claude/settings.json` always exists. The spec needs an explicit
   migration plan for these fixtures (rewrite, parametrize over engines, or
   keep claude-default and add copilot scenarios alongside).

The corresponding open questions in the master ticket
[`research/tickets/2026-05-01-510-aipm-toml-engines.md`](2026-05-01-510-aipm-toml-engines.md)
overlap with #1, #2, and #4 above; #3, #5, #6, #7 are surfaced fresh by this
research.
