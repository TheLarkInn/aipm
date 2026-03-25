---
date: 2026-03-25 03:40:04 PDT
researcher: Claude
git_commit: dd0ee787a0d3175760fde58df96e3f2ca839c619
branch: fix/marketplace-description-mismatch
repository: aipm
topic: "How does aipm init set the marketplace name, and what code paths need to change to allow customization?"
tags: [research, codebase, init, wizard, marketplace, naming]
status: complete
last_updated: 2026-03-25
last_updated_by: Claude
---

# Research: Marketplace Name Customization in `aipm init`

## Research Question

How does `aipm init` currently initialize a marketplace (including the wizard flow, marketplace.json structure, and default naming), and what code paths would need to change to allow users to customize the marketplace name during initialization?

## Summary

The marketplace name `"local-repo-plugins"` is hardcoded as a string literal in **14+ locations** across the codebase with no shared constant or configuration. It is embedded in `generate_marketplace_json()`, the `generate_scaffold_script()` TypeScript template, and the Claude Code adaptor's `settings.json` generation (both fresh-file and merge paths). The `Options` struct that carries init configuration has no field for a marketplace name, and the interactive wizard currently asks only two questions (setup mode and starter-plugin inclusion). The original spec explicitly deferred marketplace name configurability as a "follow-up."

To add customization, changes are needed across four layers: CLI arguments, the wizard prompt flow, the `Options` struct, and all downstream consumers of the name.

## Detailed Findings

### 1. Current Init Flow Architecture

The init command follows a three-layer architecture:

```
CLI (main.rs) → Wizard (wizard.rs / wizard_tty.rs) → Library (workspace_init/mod.rs)
```

**Entry point**: [`main.rs:79`](https://github.com/TheLarkInn/aipm/blob/dd0ee787a0d3175760fde58df96e3f2ca839c619/crates/aipm/src/main.rs#L79) destructures the `Commands::Init` clap variant, determines interactivity, calls the wizard, then invokes the library.

**Wizard**: Split into pure logic ([`wizard.rs`](https://github.com/TheLarkInn/aipm/blob/dd0ee787a0d3175760fde58df96e3f2ca839c619/crates/aipm/src/wizard.rs)) and a thin TTY bridge ([`wizard_tty.rs`](https://github.com/TheLarkInn/aipm/blob/dd0ee787a0d3175760fde58df96e3f2ca839c619/crates/aipm/src/wizard_tty.rs)). The pure layer builds prompt steps and resolves answers; the TTY layer calls `inquire` APIs.

**Library**: [`workspace_init::init()`](https://github.com/TheLarkInn/aipm/blob/dd0ee787a0d3175760fde58df96e3f2ca839c619/crates/libaipm/src/workspace_init/mod.rs#L104) receives an `Options` struct, calls `scaffold_marketplace()`, then applies tool adaptors.

### 2. The `Options` Struct (No Marketplace Name Field)

Defined at [`workspace_init/mod.rs:37-48`](https://github.com/TheLarkInn/aipm/blob/dd0ee787a0d3175760fde58df96e3f2ca839c619/crates/libaipm/src/workspace_init/mod.rs#L37):

```rust
pub struct Options<'a> {
    pub dir: &'a Path,
    pub workspace: bool,
    pub marketplace: bool,
    pub no_starter: bool,
    pub manifest: bool,
}
```

There is no `marketplace_name` field. The name is determined entirely by hardcoded literals downstream.

### 3. Where `"local-repo-plugins"` Is Hardcoded

#### `generate_marketplace_json()` — [`mod.rs:456-488`](https://github.com/TheLarkInn/aipm/blob/dd0ee787a0d3175760fde58df96e3f2ca839c619/crates/libaipm/src/workspace_init/mod.rs#L456)

The function takes only `no_starter: bool` and returns a JSON string with `"name": "local-repo-plugins"` embedded in both branches (lines 459 and 471). No parameter or constant controls the name.

#### `generate_scaffold_script()` — [`mod.rs:366, 398`](https://github.com/TheLarkInn/aipm/blob/dd0ee787a0d3175760fde58df96e3f2ca839c619/crates/libaipm/src/workspace_init/mod.rs#L366)

The generated TypeScript scaffold script embeds the name as a fallback marketplace object (`name: "local-repo-plugins"` at line 366) and in the `enabledPlugins` key format (`` `${name}@local-repo-plugins` `` at line 398).

#### Claude Code adaptor — [`adaptors/claude.rs`](https://github.com/TheLarkInn/aipm/blob/dd0ee787a0d3175760fde58df96e3f2ca839c619/crates/libaipm/src/workspace_init/adaptors/claude.rs)

The name appears in **8 locations** across the adaptor:

| Line | Context |
|------|---------|
| 32 | Fresh settings.json template (no_starter branch) — `"local-repo-plugins"` as key under `extraKnownMarketplaces` |
| 43 | Fresh settings.json template (default branch) — same |
| 51 | `"starter-aipm-plugin@local-repo-plugins"` in `enabledPlugins` |
| 80 | `ekm.get("local-repo-plugins")` — existence check in merge path |
| 91 | `ep.contains_key("starter-aipm-plugin@local-repo-plugins")` — enabled check |
| 107 | `ekm_obj.entry("local-repo-plugins")` — insert-if-absent |
| 112 | `serde_json::json!({ "local-repo-plugins": marketplace_entry })` — create whole section |
| 121 | `.entry("starter-aipm-plugin@local-repo-plugins")` — insert enabled entry |

#### `ToolAdaptor` trait — [`mod.rs:17-34`](https://github.com/TheLarkInn/aipm/blob/dd0ee787a0d3175760fde58df96e3f2ca839c619/crates/libaipm/src/workspace_init/mod.rs#L17)

The trait's `apply` signature is `fn apply(&self, dir: &Path, no_starter: bool, fs: &dyn Fs)`. There is no marketplace-name parameter. The adaptor hardcodes the name internally.

### 4. The Interactive Wizard (Currently No Name Prompt)

The wizard at [`wizard.rs:59-95`](https://github.com/TheLarkInn/aipm/blob/dd0ee787a0d3175760fde58df96e3f2ca839c619/crates/aipm/src/wizard.rs#L59) asks at most **two questions**:

1. **"What would you like to set up?"** — a `Select` prompt with three options (marketplace only, workspace only, both). Shown only when neither `--workspace` nor `--marketplace` flag is set.
2. **"Include starter plugin?"** — a `Confirm` prompt. Shown only when marketplace is possible and `--no-starter` was not set.

The `PromptKind` enum in the `aipm` wizard has only `Select` and `Confirm` variants — no `Text` variant for free-form input.

The resolved output is a `(bool, bool, bool)` tuple for `(workspace, marketplace, no_starter)`.

### 5. Reference Pattern: `aipm-pack` Wizard Text Input

The `aipm-pack` wizard at [`crates/aipm-pack/src/wizard.rs`](https://github.com/TheLarkInn/aipm/blob/dd0ee787a0d3175760fde58df96e3f2ca839c619/crates/aipm-pack/src/wizard.rs) demonstrates how to add text input:

- Its `PromptKind` enum includes a `Text { placeholder: String, validate: bool }` variant (lines 36-42)
- Its `PromptAnswer` enum includes a `Text(String)` variant (lines 50-51)
- Validation is done via `validate_package_name()` (lines 168-180) which checks for lowercase alphanumeric + hyphens
- Empty input is accepted, meaning the user can press Enter to accept the placeholder default
- The TTY bridge at [`wizard_tty.rs:51-65`](https://github.com/TheLarkInn/aipm/blob/dd0ee787a0d3175760fde58df96e3f2ca839c619/crates/aipm-pack/src/wizard_tty.rs#L51) calls `inquire::Text::new()` with `.with_placeholder()` and optionally `.with_validator()`

### 6. `scaffold_marketplace()` — The Orchestrator

At [`mod.rs:177-204`](https://github.com/TheLarkInn/aipm/blob/dd0ee787a0d3175760fde58df96e3f2ca839c619/crates/libaipm/src/workspace_init/mod.rs#L177), this function:

1. Creates `.ai/` directory and `.gitignore`
2. Creates `.ai/.claude-plugin/marketplace.json` via `generate_marketplace_json(no_starter)`
3. Optionally scaffolds the starter plugin

It does not accept a marketplace name — the name flows solely through `generate_marketplace_json()`.

### 7. CLI Argument Definitions

At [`main.rs:23-47`](https://github.com/TheLarkInn/aipm/blob/dd0ee787a0d3175760fde58df96e3f2ca839c619/crates/aipm/src/main.rs#L23), the `Commands::Init` variant has these fields:

| Field | Type | Flag |
|-------|------|------|
| `yes` | `bool` | `-y` / `--yes` |
| `workspace` | `bool` | `--workspace` |
| `marketplace` | `bool` | `--marketplace` |
| `no_starter` | `bool` | `--no-starter` |
| `manifest` | `bool` | `--manifest` |
| `dir` | `PathBuf` | positional, defaults to `"."` |

There is no `--name` or `--marketplace-name` flag.

### 8. Tests That Assert `"local-repo-plugins"`

Tests that will need updating when the name becomes configurable:

**Unit tests** (`workspace_init/mod.rs`):
- `marketplace_json_with_starter_is_valid` (line 858) — asserts `"name"` is `"local-repo-plugins"`
- `marketplace_json_no_starter_has_empty_plugins` (line 883) — same assertion
- `init_marketplace_creates_marketplace_json` (line 897) — reads file, asserts name
- `init_no_starter_creates_marketplace_json_with_empty_plugins` (line 932) — same
- `scaffold_script_is_nonempty` (line 769) — asserts script contains `"local-repo-plugins"`
- `scaffold_script_registers_in_marketplace` (line 789) — same
- `scaffold_script_enables_in_settings` (line 805) — asserts `"@local-repo-plugins"`
- `scaffold_script_marketplace_name_matches_generator` (line 819) — cross-consistency check
- `init_no_starter_still_configures_tools` (line 961) — asserts settings.json contains `"local-repo-plugins"`
- `init_marketplace_with_preconfigured_claude_settings` (line 1000) — pre-creates settings with the name

**Claude adaptor tests** (`adaptors/claude.rs`, line 134+):
- Multiple tests assert `"local-repo-plugins"` in fresh and merged settings.json content

**BDD scenarios** (`workspace-init.feature`):
- Line 149: `And the marketplace.json name is "local-repo-plugins"`
- Line 157: Same assertion for `--no-starter` case

**E2E tests** (`init_e2e.rs`):
- Multiple tests parse marketplace.json and assert the name field

**Insta snapshots** (`crates/aipm/src/snapshots/`):
- Several wizard snapshots may need regeneration if return type changes

### 9. Spec Acknowledgment

The interactive wizard spec at [`specs/2026-03-22-interactive-init-wizard.md:308`](https://github.com/TheLarkInn/aipm/blob/dd0ee787a0d3175760fde58df96e3f2ca839c619/specs/2026-03-22-interactive-init-wizard.md#L308) explicitly documents this as deferred:

> **Note on marketplace name and starter plugin name:** These are currently not configurable in the `Options` struct — they are hardcoded in `workspace_init::scaffold_marketplace()`. Adding prompts for these would require expanding the `Options` struct in `libaipm`, which is out of scope for this initial version. If configurability is desired later, it can be added as a follow-up by extending `Options` with optional `marketplace_name` and `starter_name` fields.

## Code References

### Core implementation
- `crates/libaipm/src/workspace_init/mod.rs:37-48` — `Options` struct (no marketplace name field)
- `crates/libaipm/src/workspace_init/mod.rs:104-128` — `init()` function
- `crates/libaipm/src/workspace_init/mod.rs:177-204` — `scaffold_marketplace()` function
- `crates/libaipm/src/workspace_init/mod.rs:456-488` — `generate_marketplace_json()` with hardcoded name

### Wizard
- `crates/aipm/src/wizard.rs:13-46` — `PromptStep`, `PromptKind`, `PromptAnswer` types
- `crates/aipm/src/wizard.rs:59-95` — `workspace_prompt_steps()` (builds 0-2 prompts)
- `crates/aipm/src/wizard.rs:100-141` — `resolve_workspace_answers()` (maps answers to booleans)
- `crates/aipm/src/wizard_tty.rs:22-35` — `resolve()` entry point

### CLI
- `crates/aipm/src/main.rs:23-47` — `Commands::Init` clap definition
- `crates/aipm/src/main.rs:79-118` — Init dispatch and output

### Claude adaptor
- `crates/libaipm/src/workspace_init/adaptors/mod.rs:13-15` — `defaults()` factory
- `crates/libaipm/src/workspace_init/adaptors/claude.rs:14-58` — `apply()` with hardcoded name
- `crates/libaipm/src/workspace_init/adaptors/claude.rs:61-132` — `merge_claude_settings()` with hardcoded name

### ToolAdaptor trait
- `crates/libaipm/src/workspace_init/mod.rs:17-34` — Trait definition (no marketplace-name parameter)

### Scaffold script (generated TypeScript)
- `crates/libaipm/src/workspace_init/mod.rs:366` — Fallback marketplace name
- `crates/libaipm/src/workspace_init/mod.rs:398` — Plugin key format with marketplace name

### Reference pattern (aipm-pack Text prompt)
- `crates/aipm-pack/src/wizard.rs:28-43` — `PromptKind::Text` variant with placeholder and validation
- `crates/aipm-pack/src/wizard.rs:168-180` — `validate_package_name()` implementation
- `crates/aipm-pack/src/wizard_tty.rs:51-65` — `inquire::Text` execution with validator

## Architecture Documentation

### Two-Layer Wizard Pattern
The project uses a consistent wizard architecture across both `aipm` and `aipm-pack`:
1. **Pure logic layer** (`wizard.rs`) — builds `PromptStep` structs conditionally based on CLI flags, resolves `PromptAnswer` vectors into typed output. Fully testable with insta snapshots.
2. **TTY bridge** (`wizard_tty.rs`) — iterates prompt steps and calls `inquire` APIs. Excluded from coverage gate.

### Adaptor Pattern
Tool integrations are pluggable via `Box<dyn ToolAdaptor>`. The `defaults()` factory at `adaptors/mod.rs:13` currently returns only `claude::Adaptor`. The trait signature would need to accept a marketplace name for adaptors to use a custom name.

### Flag Elision
When a CLI flag already determines a value, the corresponding wizard prompt is omitted from the step list entirely (not shown as a locked/disabled prompt).

## Historical Context (from research/)

- `specs/2026-03-22-interactive-init-wizard.md` — The original wizard spec explicitly deferred marketplace name customization (line 308)
- `specs/2026-03-16-aipm-init-workspace-marketplace.md` — Original design spec for init workspace/marketplace scaffolding
- `research/docs/2026-03-22-rust-interactive-cli-prompts.md` — Research on `inquire` crate usage for interactive prompts
- `research/docs/2026-03-16-aipm-init-workspace-marketplace.md` — Research for the init feature

## Related Research

- `research/docs/2026-03-24-marketplace-description-mismatch-bug.md` — Related marketplace.json field propagation research
- `research/docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md` — Manifest generation during init/migrate

## Open Questions

1. **Validation rules**: What character set should be allowed for marketplace names? The `aipm-pack` wizard uses lowercase alphanumeric + hyphens + `@` + `/` for package names — should the same rules apply?
2. **Default value**: Should the default remain `"local-repo-plugins"`, or should it derive from the directory name (as `aipm-pack` does for package names)?
3. **Non-interactive default**: When `-y` is passed, what marketplace name should be used?
4. **Starter plugin key format**: The composite key `"starter-aipm-plugin@{marketplace-name}"` is used in Claude Code `settings.json`. If the name is customizable, this key must be dynamically constructed.
5. **Scaffold script**: The generated TypeScript in `generate_scaffold_script()` also hardcodes the name. Should the scaffold script read the name from `marketplace.json` at runtime, or should it be templated with the custom name at generation time?
6. **Migrate command**: The `migrate` module reads marketplace.json but does not reference the name by value — does it need any changes?
