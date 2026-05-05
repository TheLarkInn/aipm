---
date: 2026-05-05
researcher: Sean Larkin
git_commit: ad39977
branch: main
repository: aipm
topic: "aipm init scaffolding trace"
tags: [research, codebase, scaffolding, init]
status: complete
last_updated: 2026-05-05
last_updated_by: Sean Larkin
---

# `aipm init` Scaffolding Trace

## Overview

There are two distinct "init" code paths in this repo:

1. **`aipm init`** (consumer / workspace bootstrap) — creates `.ai/`
   marketplace, `.claude/settings.json`, optional workspace `aipm.toml`.
   Implemented in `crates/libaipm/src/workspace_init/mod.rs` plus the `claude`
   adaptor in `crates/libaipm/src/workspace_init/adaptors/claude.rs`.
2. **`aipm pack init`** (plugin-author scaffolding) — creates a single plugin
   package layout. Implemented in `crates/libaipm/src/init.rs`. **Never writes
   anything under `.claude/`.**

Issue #724 concerns path 1.

## Entry points

- `crates/aipm/src/main.rs:36-64` — `Commands::Init` clap subcommand definition
- `crates/aipm/src/main.rs:1201` —
  `Some(Commands::Init { … }) => cmd_init(&flags, manifest, name.as_deref(), dir)`
- `crates/aipm/src/main.rs:398-445` — `cmd_init()` resolves wizard answers and
  calls `libaipm::workspace_init::init(...)`
- `crates/libaipm/src/workspace_init/mod.rs:96-120` —
  `pub fn init(opts, adaptors, fs)` is the library entry point

## Wizard / default resolution

- `crates/aipm/src/main.rs:405`: `let interactive = !flags.yes && std::io::stdin().is_terminal();`
- `crates/aipm/src/wizard_tty.rs:37-51` — `resolve()` dispatches to interactive
  prompts or `resolve_defaults`
- `crates/aipm/src/wizard.rs:140-150` — `resolve_defaults()` non-interactive
  defaulting:

```rust
let (w, m) = if !workspace && !marketplace { (false, true) } else { (workspace, marketplace) };
let marketplace_name =
    name.filter(|s| !s.is_empty()).unwrap_or("local-repo-plugins").to_string();
(w, m, no_starter, marketplace_name)
```

So with no flags: `workspace=false, marketplace=true, no_starter=false,
marketplace_name="local-repo-plugins"`. `manifest=false` is the default
(separate flag).

`crates/libaipm/src/workspace_init/adaptors/mod.rs:13-15` — adaptors
unconditionally include the Claude adaptor:

```rust
pub fn defaults() -> Vec<Box<dyn ToolAdaptor>> {
    vec![Box::new(claude::Adaptor)]
}
```

This is hard-wired — no engine-selection or config flag suppresses it.

## Top-level dispatch in `init()`

`crates/libaipm/src/workspace_init/mod.rs:96-120`:

```rust
pub fn init(opts, adaptors, fs) -> Result<InitResult, Error> {
    let mut actions = Vec::new();
    if opts.workspace {
        init_workspace(opts.dir, fs)?;
        actions.push(InitAction::WorkspaceCreated);
    }
    if opts.marketplace {
        scaffold_marketplace(opts.dir, opts.no_starter, opts.manifest, opts.marketplace_name, fs)?;
        actions.push(InitAction::MarketplaceCreated);
        for adaptor in adaptors {
            if adaptor.apply(opts.dir, opts.no_starter, opts.marketplace_name, fs)? {
                actions.push(InitAction::ToolConfigured(adaptor.name().to_string()));
            }
        }
    }
    Ok(InitResult { actions })
}
```

The adaptor loop (which writes `.claude/`) is gated only on `opts.marketplace`.
There is no per-engine opt-out and no manifest-driven engine selection at this
layer.

## `.claude` references in source (init write path)

| File:line | Context | Purpose |
|---|---|---|
| `crates/libaipm/src/workspace_init/adaptors/claude.rs:26` | `let settings_dir = dir.join(".claude");` | Builds the `.claude` directory path used as the workspace tool-config root |
| `crates/libaipm/src/workspace_init/adaptors/claude.rs:27` | `let settings_path = settings_dir.join("settings.json");` | Builds `.claude/settings.json` write target |
| `crates/libaipm/src/workspace_init/adaptors/claude.rs:29` | `fs.create_dir_all(&settings_dir)?;` | Creates `<dir>/.claude` (unconditional once `apply()` runs) |
| `crates/libaipm/src/workspace_init/mod.rs:202` | `fs.create_dir_all(&ai_dir.join(".claude-plugin"))?;` | Creates `.ai/.claude-plugin/` (marketplace registry directory inside `.ai/`, not `.claude/`) |
| `crates/libaipm/src/workspace_init/mod.rs:212` | `&ai_dir.join(".claude-plugin").join("marketplace.json")` | Writes `.ai/.claude-plugin/marketplace.json` |
| `crates/libaipm/src/workspace_init/mod.rs:223` | `fs.create_dir_all(&starter.join(".claude-plugin"))?;` | Creates `.ai/starter-aipm-plugin/.claude-plugin/` |
| `crates/libaipm/src/workspace_init/mod.rs:268` | `fs.write_file(&starter.join(".claude-plugin").join("plugin.json"), …)` | Writes `.ai/starter-aipm-plugin/.claude-plugin/plugin.json` |

There is no path-constant indirection for `.claude/`: the literal string is
hard-coded at `claude.rs:26`. The repo does have
`libaipm_engine_spec::paths::CLAUDE_DOT = ".claude"` (defined in
`crates/libaipm-engine-spec/build.rs:363`), but the workspace_init Claude
adaptor does **not** use it.

## All `create_dir_all` calls in the init path

(test code in `#[cfg(test)]` modules excluded.)

| File:line | Path argument | Gated by |
|---|---|---|
| `crates/libaipm/src/workspace_init/mod.rs:138` | `dir` (workspace root) | `opts.workspace` |
| `crates/libaipm/src/workspace_init/mod.rs:187` | `&ai_dir` (= `dir.join(".ai")`) | `opts.marketplace` |
| `crates/libaipm/src/workspace_init/mod.rs:202` | `ai_dir.join(".claude-plugin")` | `opts.marketplace` |
| `crates/libaipm/src/workspace_init/mod.rs:223` | `starter.join(".claude-plugin")` | `opts.marketplace && !opts.no_starter` |
| `crates/libaipm/src/workspace_init/mod.rs:224` | `starter.join("skills").join("scaffold-plugin")` | same |
| `crates/libaipm/src/workspace_init/mod.rs:225` | `starter.join("scripts")` | same |
| `crates/libaipm/src/workspace_init/mod.rs:226` | `starter.join("agents")` | same |
| `crates/libaipm/src/workspace_init/mod.rs:227` | `starter.join("hooks")` | same |
| `crates/libaipm/src/workspace_init/adaptors/claude.rs:29` | `dir.join(".claude")` | `opts.marketplace` (unconditional inside `apply()`) |
| `crates/aipm/src/main.rs:539` | `&plugins_dir` | `cmd_install` (not `init`) |

## Full sequence of files / directories produced by `aipm init`

After wizard / `resolve_defaults` (non-interactive default = `workspace=false,
marketplace=true, no_starter=false, manifest=false`):

### Phase 1 — Workspace manifest (only if `opts.workspace == true`)

`fn init_workspace` — `crates/libaipm/src/workspace_init/mod.rs:126-142`:

1. Pre-check: `if fs.exists(&dir.join("aipm.toml"))` → return
   `WorkspaceAlreadyInitialized` (line 128-130)
2. `generate_workspace_manifest()` (lines 144-168) — TOML with
   `members = [".ai/*"]`, `plugins_dir = ".ai"`
3. Round-trip validate via `crate::manifest::parse_and_validate` (line 135)
4. `fs.create_dir_all(dir)?;` (line 138)
5. `fs.write_file(&manifest_path, content.as_bytes())?;` — writes
   `<dir>/aipm.toml` (line 139)

### Phase 2 — Marketplace scaffold (only if `opts.marketplace == true`)

`fn scaffold_marketplace` — `crates/libaipm/src/workspace_init/mod.rs:174-274`:

1. **Pre-check** (line 182-184): `if fs.exists(&ai_dir)` → return
   `MarketplaceAlreadyExists`
2. `fs.create_dir_all(&ai_dir)?;` — creates `.ai/` (line 187)
3. `fs.write_file(&ai_dir.join(".gitignore"), gitignore_content.as_bytes())?;`
   — writes `.ai/.gitignore`. Body depends on `no_starter`
4. `fs.create_dir_all(&ai_dir.join(".claude-plugin"))?;` — creates
   `.ai/.claude-plugin/` (line 202)
5. `fs.write_file(&ai_dir.join(".claude-plugin").join("marketplace.json"), …)`
   — writes `.ai/.claude-plugin/marketplace.json`
6. **Early return** at line 216-218: `if no_starter { return Ok(()); }` — phase
   2 ends here when `--no-starter` is used
7. (only if `!no_starter`) Create starter plugin tree (lines 220-227):
   `.ai/starter-aipm-plugin/.claude-plugin/`,
   `.ai/starter-aipm-plugin/skills/scaffold-plugin/`,
   `.ai/starter-aipm-plugin/scripts/`,
   `.ai/starter-aipm-plugin/agents/`,
   `.ai/starter-aipm-plugin/hooks/`
8. Write component files (lines 230-242):
   `.ai/starter-aipm-plugin/skills/scaffold-plugin/SKILL.md`,
   `scripts/scaffold-plugin.sh`, `agents/marketplace-scanner.md`,
   `hooks/hooks.json`
9. (only if `manifest == true`) Lines 245-252: write
   `.ai/starter-aipm-plugin/aipm.toml` ← `generate_starter_manifest()`
10. Write `.ai/starter-aipm-plugin/.claude-plugin/plugin.json` (lines 255-268)
    — unconditional within `!no_starter`
11. Write `.ai/starter-aipm-plugin/.mcp.json` ← `generate_mcp_stub()` (line 271)

### Phase 3 — Adaptors loop (only if `opts.marketplace == true`)

`crates/libaipm/src/workspace_init/mod.rs:112-116`:

```rust
for adaptor in adaptors {
    if adaptor.apply(opts.dir, opts.no_starter, opts.marketplace_name, fs)? {
        actions.push(InitAction::ToolConfigured(adaptor.name().to_string()));
    }
}
```

Adaptor list comes from `adaptors::defaults()` which returns
`vec![Box::new(claude::Adaptor)]` — single adaptor, always Claude.

Inside `claude::Adaptor::apply` (`adaptors/claude.rs:19-69`):

1. Line 26: `let settings_dir = dir.join(".claude");`
2. Line 27: `let settings_path = settings_dir.join("settings.json");`
3. Line 29: `fs.create_dir_all(&settings_dir)?;` — **creates `<dir>/.claude/`
   unconditionally on every call**, even when the resulting settings change-set
   is empty (the directory is created before the early-return at line 67 that
   returns `Ok(false)`)
4. Lines 33-40: read existing `.claude/settings.json` or default to `{}` on
   `NotFound`
5. Lines 42-51: reject non-object root
6. Line 53-54: `add_known_marketplace(&mut settings, marketplace_name)` mutates
   in-memory JSON
7. Lines 56-61: gated on `no_starter` —

```rust
let ep_changed = if no_starter {
    false
} else {
    let starter_key = format!("starter-aipm-plugin@{marketplace_name}");
    crate::generate::settings::enable_plugin(&mut settings, &starter_key)
};
```

8. Lines 63-68: only writes `.claude/settings.json` if either marketplace or
   enabledPlugins actually changed. Returns `Ok(false)` and skips writing
   otherwise (but the directory created at step 3 remains)

## Embedded template strings

No `include_str!` / `include_bytes!` calls in `workspace_init/`. All templates
are inline string literals returned from helpers in
`crates/libaipm/src/workspace_init/mod.rs`:

| Helper (file:line) | Destination |
|---|---|
| `generate_workspace_manifest()` (144-168) — calls `crate::manifest::builder::build_workspace_manifest` | `<dir>/aipm.toml` (line 139) |
| `generate_starter_manifest()` (276-299) — calls `crate::manifest::builder::build_plugin_manifest` with `engines: Some(&["claude"])` | `.ai/starter-aipm-plugin/aipm.toml` (line 247, gated on `manifest`) |
| `generate_skill_template()` (301-327) | `.ai/starter-aipm-plugin/skills/scaffold-plugin/SKILL.md` |
| `generate_scaffold_script()` (329-336) — runs `aipm make plugin --name … --engine "${2:-claude}"` | `.ai/starter-aipm-plugin/scripts/scaffold-plugin.sh` |
| `generate_agent_template()` (338-368) | `.ai/starter-aipm-plugin/agents/marketplace-scanner.md` |
| `generate_hook_template()` (370-380) — appending to `.ai/.tool-usage.log` | `.ai/starter-aipm-plugin/hooks/hooks.json` |
| `generate_mcp_stub()` (382-384) — `{"mcpServers": {}}` | `.ai/starter-aipm-plugin/.mcp.json` |
| `crate::generate::marketplace::create(name, plugins)` | `.ai/.claude-plugin/marketplace.json` |
| `crate::generate::plugin_json::generate(opts, components)` | `.ai/starter-aipm-plugin/.claude-plugin/plugin.json` |

The starter manifest at line 282 hard-codes engine selection:

```rust
let starter_engines: &[&str] = &["claude"];
```

## Conditional vs unconditional matrix

Default flags (`workspace=false, marketplace=true, no_starter=false,
manifest=false`):

| Path | Created? | Gate |
|---|---|---|
| `<dir>/aipm.toml` | only with `--workspace` | `if opts.workspace` (`mod.rs:103`) |
| `<dir>/.ai/` | yes by default | `if opts.marketplace` (`mod.rs:108`) |
| `<dir>/.ai/.gitignore` | yes by default | unconditional inside `scaffold_marketplace` (line 199) |
| `<dir>/.ai/.claude-plugin/` | yes by default | unconditional (line 202) |
| `<dir>/.ai/.claude-plugin/marketplace.json` | yes by default | unconditional (line 211) |
| `<dir>/.ai/starter-aipm-plugin/...` | yes by default | `!no_starter` early-return (line 216-218) |
| starter `SKILL.md`, `scaffold-plugin.sh`, etc. | yes by default | same `!no_starter` gate |
| `.ai/starter-aipm-plugin/aipm.toml` | only with `--manifest` | `if manifest` (line 245-252) |
| **`<dir>/.claude/`** | **yes by default — UNCONDITIONAL once marketplace adaptor loop runs** | gated only on `opts.marketplace`. Inside the adaptor, `fs.create_dir_all(&settings_dir)?;` (line 29) is **unconditional**. No `if engine == claude` / no manifest field consulted |
| `<dir>/.claude/settings.json` | yes by default | conditional inside the adaptor: `if mp_changed || ep_changed { … write(...)?; }` |

The verbatim engine-selection point — the entire engine selection lives here:

```rust
// crates/libaipm/src/workspace_init/adaptors/mod.rs:13-15
pub fn defaults() -> Vec<Box<dyn ToolAdaptor>> {
    vec![Box::new(claude::Adaptor)]
}
```

The unconditional `.claude/` creation:

```rust
// crates/libaipm/src/workspace_init/adaptors/claude.rs:26-29
let settings_dir = dir.join(".claude");
let settings_path = settings_dir.join("settings.json");

fs.create_dir_all(&settings_dir)?;
```

Confirmed by the test at `workspace_init/mod.rs:868-905`
(`init_no_starter_still_configures_tools`):

```rust
// Tool settings should still be applied
assert!(tmp.join(".claude/settings.json").exists());
// But no starter plugin
assert!(!tmp.join(".ai/starter-aipm-plugin").exists());
```

I.e. `--no-starter` does **not** suppress `.claude/`.

## CLI flag → behavior summary

- Default (`aipm init`): `.claude/settings.json` is created
- `aipm init --workspace` (alone): `.claude/` is **not** created — adaptor loop
  skipped (gate at `mod.rs:108`)
- `aipm init --marketplace`: `.claude/settings.json` created
- `aipm init --workspace --marketplace`: `.claude/settings.json` created
- `aipm init --no-starter`: `.claude/settings.json` still created (only
  `enabledPlugins[starter…]` is suppressed)
- No `--no-claude` / `--engine` / `--skip-tool-config` flag exists in
  `Commands::Init`
- The `[engines]` field of `aipm.toml` is **not** read in this code path

## Tests that exercise scaffolding

### Cucumber `.feature` files

- `tests/features/manifest/workspace-init.feature` — 23 scenarios. Glue is in
  `crates/libaipm/tests/bdd.rs`. Hard-coded `.claude/settings.json` expectations
  at lines 81-85 and 173-186.

```gherkin
# tests/features/manifest/workspace-init.feature:81-85
Scenario: No-starter flag still configures tool settings
  Given an empty directory "my-project"
  When the user runs "aipm init --no-starter" in "my-project"
  Then a file ".claude/settings.json" exists in "my-project"
  And there is no directory ".ai/starter-aipm-plugin" in "my-project"
```

BDD step glue: `crates/libaipm/tests/bdd.rs:594-650`.

### Rust integration tests

- `crates/aipm/tests/init_e2e.rs`:
  - `init_default_creates_marketplace_only` (line 22) —
    `assert!(dir.join(".claude/settings.json").exists(), …);` (line 38)
  - `init_claude_settings_generated` (line 135)
  - `init_settings_json_marketplace_name_and_enabled_plugins` (line 205)
  - `scaffold_script_enables_in_settings_json` (line 298)
  - `scaffold_script_multiple_plugins_no_duplicates` (line 342)

### Library unit tests inside `workspace_init`

- `crates/libaipm/src/workspace_init/mod.rs:868-905` —
  `init_no_starter_still_configures_tools`
- `mod.rs:907-936` — `init_marketplace_with_preconfigured_claude_settings`
- `mod.rs:1255-1291` — `init_adaptor_error_propagates`
- `mod.rs:1293-1337` — `adaptor_apply_returns_false_when_already_configured`
- `mod.rs:1359-1396` — `init_adaptor_skips_when_settings_already_configured`
- `mod.rs:585-594` — `init_with_no_adaptors`: passes empty adaptor vec and
  asserts `assert!(!tmp.join(".claude").exists());` (line 591). **The only test
  that demonstrates `.claude/` can be suppressed — but only by passing an empty
  `adaptors` slice into the library API, which the CLI never does.**
- `crates/libaipm/src/workspace_init/adaptors/claude.rs:90-137` and 140-310 —
  six tests exercising the Claude adaptor's create/merge/skip paths

### Snapshot

`crates/libaipm/src/workspace_init/snapshots/libaipm__workspace_init__tests__scaffold_script_snapshot.snap`
— locks the bash script content from `generate_scaffold_script()`.

## Code references

- `crates/aipm/src/main.rs` (init clap def 36-64; `cmd_init` 398-445; dispatch 1201)
- `crates/aipm/src/wizard.rs` (workspace_prompt_steps, resolve_workspace_answers,
  resolve_defaults — lines 23-150)
- `crates/aipm/src/wizard_tty.rs` (resolve — lines 37-51)
- `crates/libaipm/src/workspace_init/mod.rs` (entire scaffolding pipeline)
- `crates/libaipm/src/workspace_init/error.rs`
- `crates/libaipm/src/workspace_init/adaptors/mod.rs:13-15` — hardcoded Claude
  default
- `crates/libaipm/src/workspace_init/adaptors/claude.rs` — Claude adaptor
- `crates/libaipm/src/generate/settings.rs` — `add_known_marketplace`,
  `enable_plugin`, `write`
- `crates/libaipm/src/generate/marketplace.rs` — `create`
- `crates/libaipm/src/generate/plugin_json.rs`
- `crates/libaipm/src/init.rs` — separate `aipm pack init` (does **not** touch
  `.claude/`)
- `crates/libaipm-engine-spec/build.rs:363` — defines `paths::CLAUDE_DOT` (not
  used by workspace_init)
- `tests/features/manifest/workspace-init.feature`
- `crates/libaipm/tests/bdd.rs:459-691` — BDD step glue for workspace init
- `crates/aipm/tests/init_e2e.rs`
