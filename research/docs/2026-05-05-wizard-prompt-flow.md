---
date: 2026-05-05
researcher: Sean Larkin
git_commit: ad39977
branch: main
repository: aipm
topic: "aipm init wizard prompt flow"
tags: [research, codebase, wizard, init]
status: complete
last_updated: 2026-05-05
last_updated_by: Sean Larkin
---

# `aipm init` Wizard Prompt Flow (current)

## Overview

The `aipm init` wizard is implemented across two layers: a **definition layer**
(`crates/aipm/src/wizard.rs`) that builds prompt configs as pure data, and a
**TTY bridge** (`crates/aipm/src/wizard_tty.rs`) that calls into
`libaipm::wizard::execute_prompts()` to render them via `inquire`. Shared types
(`PromptStep`, `PromptKind`, `PromptAnswer`, `execute_prompts`) live in
`crates/libaipm/src/wizard.rs` behind the `wizard` feature flag.

The wizard does **not** ask about engine. Engine is hardcoded via the adaptor
list in `crates/libaipm/src/workspace_init/adaptors/mod.rs:13-15`, which always
returns the Claude adaptor. `ENGINE_OPTIONS` exists in
`crates/aipm/src/wizard.rs:297` but is consumed only by `aipm make plugin`, not
by `aipm init`.

## Entry points

- `crates/aipm/src/main.rs:1201` — top-level dispatch for `Commands::Init`
- `crates/aipm/src/main.rs:398` — `fn cmd_init(...)` handler
- `crates/aipm/src/wizard_tty.rs:37` — `pub fn resolve(...)` — interactive vs
  non-interactive switch
- `crates/aipm/src/wizard.rs:23` — `pub fn workspace_prompt_steps(...)` — builds
  prompt list

## CLI surface for `aipm init`

Defined at `crates/aipm/src/main.rs:35-64`. Flags:

| Flag | Purpose |
|---|---|
| `-y`, `--yes` (38-39) | Skip prompts, use defaults |
| `--workspace` (42-43) | Generate workspace `aipm.toml` |
| `--marketplace` (46-47) | Generate `.ai/` marketplace + tool settings |
| `--no-starter` (50-51) | Skip starter plugin |
| `--manifest` (54-55) | Generate `aipm.toml` plugin manifests |
| `--name` (58-59) | Custom marketplace name |
| `dir` positional (62-63) | Directory, defaults to `.` |

There is **no `--engine` flag on `init`**. (The `--engine` flag at
`main.rs:84-85` belongs to `Install`, not `Init`.)

## Interactive vs non-interactive decision

`crates/aipm/src/main.rs:405`:

```rust
let interactive = !flags.yes && std::io::stdin().is_terminal();
```

Interactive mode requires both: `--yes` not passed AND stdin is a TTY. Either
condition false → non-interactive.

## The three prompts (in order)

Built by `workspace_prompt_steps()` at `crates/aipm/src/wizard.rs:23-73`.
Captured exactly in snapshot
`crates/aipm/src/snapshots/aipm__wizard__tests__workspace_prompts_no_flags_snapshot.snap`.

### Prompt 1 — Setup mode (Select)

Source: `wizard.rs:36-41`. Conditional: shown only when
`!flag_workspace && !flag_marketplace` (line 33).

- **Label**: `"What would you like to set up?"`
- **Type**: Single-select (`PromptKind::Select`)
- **Options** (`SETUP_OPTIONS`, `wizard.rs:17-18`):
  - `[0] "Marketplace only (recommended)"` — default
  - `[1] "Workspace manifest only"`
  - `[2] "Both workspace + marketplace"`
- **Default index**: `0`
- **Help**: `"Use arrow keys, Enter to select"`
- **Validation**: none (free choice from list)
- **Storage**: index → `PromptAnswer::Selected(usize)`. Mapped at
  `wizard.rs:89-99`:
  - `Selected(1)` → `(workspace=true, marketplace=false)`
  - `Selected(2)` → `(workspace=true, marketplace=true)`
  - default branch → `(workspace=false, marketplace=true)`

### Prompt 2 — Marketplace name (Text)

Source: `wizard.rs:53-61`. Conditional: shown only if
`marketplace_possible && !has_name` where
`marketplace_possible = flag_marketplace || needs_setup_prompt` (line 48) and
`has_name = flag_name.is_some_and(|s| !s.is_empty())` (line 51).

- **Label**: `"Marketplace name:"`
- **Type**: Text input (`PromptKind::Text`)
- **Placeholder**: `"local-repo-plugins"`
- **Help**: `"Lowercase alphanumeric with hyphens, or press Enter for default"`
- **Validation**: `validate: true` triggers `inquire` validator at
  `crates/libaipm/src/wizard.rs:113-123` which calls
  `crate::manifest::validate::check_name(input, ValidationMode::Interactive)`.
  Empty string is accepted (matches test
  `validate_marketplace_name_accepts_empty_for_default` at `wizard.rs:699-701`).
- **Storage**: `PromptAnswer::Text(String)`. Resolved at `wizard.rs:103-117`:
  empty input falls back to `"local-repo-plugins"`; non-empty becomes
  `marketplace_name`.

### Prompt 3 — Include starter plugin (Confirm)

Source: `wizard.rs:64-70`. Conditional: shown only if
`marketplace_possible && !flag_no_starter` (line 64).

- **Label**: `"Include starter plugin?"`
- **Type**: Yes/no confirm (`PromptKind::Confirm`)
- **Default**: `true`
- **Help**: `"Adds scaffold-plugin skill, marketplace-scanner agent, and logging hook"`
- **Validation**: none (boolean)
- **Storage**: `PromptAnswer::Bool(bool)`. Mapped at `wizard.rs:120-128`: if
  `do_marketplace` is true and answer is `Bool(include)`, then
  `no_starter = !include`. Otherwise `no_starter` stays at `flag_no_starter`.

## Engine prompt — DOES NOT EXIST in `aipm init`

Confirmed by reading the full prompt list at `crates/aipm/src/wizard.rs:23-73`
— only the three prompts above are emitted.

The engine list `ENGINE_OPTIONS` does exist at `crates/aipm/src/wizard.rs:297`:

```rust
pub const ENGINE_OPTIONS: &[&str] = &["Claude Code", "Copilot CLI", "Both"];
```

…but it is consumed only by the `aipm make plugin` wizard
(`wizard_tty.rs:113-119` and `wizard.rs:319-325` "Target engine" prompt), **not**
by `aipm init`.

The Claude folder is generated unconditionally by the adaptor list at
`crates/libaipm/src/workspace_init/adaptors/mod.rs:13-15`:

```rust
pub fn defaults() -> Vec<Box<dyn ToolAdaptor>> {
    vec![Box::new(claude::Adaptor)]
}
```

Doc comment at lines 11-12: *"Currently only includes Claude Code. Future
adaptors (Copilot CLI, OpenCode, etc.) are added here."*

`cmd_init` calls `libaipm::workspace_init::adaptors::defaults()` at
`main.rs:413` and passes the resulting `Vec<Box<dyn ToolAdaptor>>` to
`libaipm::workspace_init::init(...)` at `main.rs:423`. Inside `init()`
(`workspace_init/mod.rs:96-120`), if `opts.marketplace` is true, the loop at
lines 112-116 calls every adaptor's `apply()` method. The
`claude::Adaptor::apply()` implementation at
`crates/libaipm/src/workspace_init/adaptors/claude.rs:19-69` unconditionally
creates `dir.join(".claude")` (line 26) and writes `settings.json`.

## Handoff to library: `Options` struct

`cmd_init` at `crates/aipm/src/main.rs:407-423` constructs a
`libaipm::workspace_init::Options` struct (not an `aipm.toml` in memory) and
calls `libaipm::workspace_init::init()`:

```rust
let (do_workspace, do_marketplace, do_no_starter, marketplace_name) = wizard_tty::resolve(
    interactive,
    (flags.workspace, flags.marketplace, flags.no_starter),
    name,
)?;

let adaptors = libaipm::workspace_init::adaptors::defaults();
let opts = libaipm::workspace_init::Options {
    dir: &dir,
    workspace: do_workspace,
    marketplace: do_marketplace,
    no_starter: do_no_starter,
    manifest,
    marketplace_name: &marketplace_name,
};

let result = libaipm::workspace_init::init(&opts, &adaptors, &libaipm::fs::Real)?;
```

`Options` definition at `crates/libaipm/src/workspace_init/mod.rs:52-65`. **No
engine field.** The 4-tuple `(workspace, marketplace, no_starter,
marketplace_name)` returned by the wizard is the entire surface — there is no
in-memory `aipm.toml` value passed.

The actual TOML content is generated downstream by `generate_workspace_manifest()`
at `workspace_init/mod.rs:144-168` (workspace manifest) and
`generate_starter_manifest()` at lines 276-299. The starter manifest hardcodes
engines at line 282:

```rust
let starter_engines: &[&str] = &["claude"];
```

## Non-interactive path

`crates/aipm/src/wizard_tty.rs:42-50`:

```rust
if interactive {
    inquire::set_global_render_config(styled_render_config());
    let steps = workspace_prompt_steps(workspace, marketplace, no_starter, flag_name);
    let answers = libaipm::wizard::execute_prompts(&steps)?;
    Ok(resolve_workspace_answers(&answers, workspace, marketplace, no_starter, flag_name))
} else {
    Ok(resolve_defaults(workspace, marketplace, no_starter, flag_name))
}
```

`resolve_defaults` at `crates/aipm/src/wizard.rs:140-150`:

- If neither `--workspace` nor `--marketplace` set →
  `(workspace=false, marketplace=true)` (marketplace only)
- Otherwise pass through both flags
- `no_starter` flag passed through
- `marketplace_name`: filter empty → `"local-repo-plugins"` default, else use
  flag value

## Prompt execution layer

`crates/libaipm/src/wizard.rs:78-148` — `execute_prompts(&[PromptStep])`
dispatches each step to an `inquire::*` prompt:

- `PromptKind::Select` → `inquire::Select` (lines 85-99)
- `PromptKind::Confirm` → `inquire::Confirm` (lines 100-107)
- `PromptKind::Text` → `inquire::Text`, with optional validator binding
  `manifest::validate::check_name` (lines 108-126)
- `PromptKind::MultiSelect` → `inquire::MultiSelect` (lines 127-142)

Returns `Vec<PromptAnswer>` — one answer per step in order.

## Render config / theming

`crates/libaipm/src/wizard.rs:62-70` — `styled_render_config()` builds
`inquire::ui::RenderConfig`:

- `prompt_prefix`: cyan `?`
- `answered_prompt_prefix`: green `✓` (`\u{2713}`)
- `placeholder`: dark grey

Set globally in `wizard_tty.rs:44` via
`inquire::set_global_render_config(...)` before each prompt sequence.

## Tests

### Unit / snapshot tests (in same file as wizard logic)

`crates/aipm/src/wizard.rs:411-996` — module `mod tests` with 49 tests covering:

- Prompt step snapshots for flag combinations:
  - `workspace_prompts_no_flags_snapshot` (line 463)
  - `workspace_prompts_workspace_flag_snapshot` (line 469)
  - `workspace_prompts_marketplace_flag_snapshot` (line 475)
  - `workspace_prompts_both_flags_snapshot` (line 481)
  - `workspace_prompts_no_starter_flag_snapshot` (line 487)
  - `workspace_prompts_all_flags_snapshot` (line 493)
  - `workspace_prompts_name_flag_omits_name_prompt` (line 499)
  - `workspace_prompts_workspace_only_omits_name_prompt` (line 505)
- Answer resolution snapshots: lines 516-616
- `resolve_defaults` cases: lines 636-675
- Marketplace name validation: lines 681-721
- Migrate cleanup prompts: lines 727-752
- Pack init prompts: lines 773-893
- Make plugin prompts: lines 899-966

Snapshot files at `crates/aipm/src/snapshots/aipm__wizard__tests__*.snap` (29+
files).

### Library-layer wizard tests

`crates/libaipm/src/wizard.rs:150-205` — `mod tests` covers `PromptStep` debug,
`PromptKind` variants, `PromptAnswer` equality, `styled_render_config`, and
`execute_prompts` with empty steps.

### BDD tests for `aipm init`

`tests/features/manifest/workspace-init.feature` — 30+ scenarios. All scenarios
pass explicit CLI flags; none exercise the interactive prompt flow. The
scenario at lines 140-147 ("Default init with no flags creates marketplace
only") exercises the non-interactive default branch. Tests for the Claude
adaptor running unconditionally are at lines 174-192 (rule "Tool settings
integration").

`tests/features/manifest/init.feature` — covers `aipm make plugin`, not
`aipm init`.

### Coverage exclusion

`wizard_tty.rs` is excluded from coverage via the `--ignore-filename-regex`
pattern documented in `CLAUDE.md` because it requires a real TTY. The pure-data
layer at `crates/aipm/src/wizard.rs` is fully covered.

## Data flow summary

1. User invokes `aipm init [flags] [dir]` → `main.rs:1201`
2. `cmd_init` resolves directory and computes `interactive` boolean →
   `main.rs:404-405`
3. `wizard_tty::resolve()` either runs prompts or applies defaults →
   `wizard_tty.rs:37-51`
4. **Interactive**: `workspace_prompt_steps()` builds `Vec<PromptStep>` →
   `wizard.rs:23-73`
5. `libaipm::wizard::execute_prompts()` renders prompts via `inquire` →
   `libaipm/src/wizard.rs:78-148`
6. `resolve_workspace_answers()` maps `Vec<PromptAnswer>` →
   `(bool, bool, bool, String)` → `wizard.rs:78-131`
7. `cmd_init` builds `libaipm::workspace_init::Options` (no engine field) →
   `main.rs:414-421`
8. `libaipm::workspace_init::init()` runs adaptors loop with hardcoded
   `defaults()` (Claude only) → `workspace_init/mod.rs:96-120`
9. `claude::Adaptor::apply()` always creates `.claude/settings.json` when
   `marketplace=true` → `workspace_init/adaptors/claude.rs:19-69`
10. `InitResult` actions formatted as messages and printed →
    `main.rs:425-443`

## Code references

- `crates/aipm/src/wizard_tty.rs` — TTY bridge (~206 lines)
- `crates/aipm/src/wizard.rs` — prompt definitions and tests (~996 lines)
- `crates/libaipm/src/wizard.rs` — shared types and execution (~205 lines)
- `crates/aipm/src/main.rs:35-64, 385-445, 1200-1204`
- `crates/libaipm/src/workspace_init/mod.rs:51-120` — `Options` and `init()`
- `crates/libaipm/src/workspace_init/adaptors/mod.rs:13-15` — hardcoded Claude
  default
- `crates/libaipm/src/workspace_init/adaptors/claude.rs:19-69` — unconditional
  `.claude/` creation
- `tests/features/manifest/workspace-init.feature` — flag-only BDD scenarios
- `crates/aipm/src/snapshots/` — 29+ wizard snapshot files
