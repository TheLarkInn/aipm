---
date: 2026-05-05
researcher: Sean Larkin
git_commit: ad39977
branch: main
repository: aipm
topic: "How `aipm init` is wired today (CLI entry point, dispatch, TTY detection, engine selection)"
tags: [research, codebase, init, cli, wizard, workspace_init, tty]
status: complete
last_updated: 2026-05-05
last_updated_by: Sean Larkin
---

# Research: `aipm init` CLI entry point and call graph

## Overview

`aipm init` is defined as a clap-derived subcommand in `crates/aipm/src/main.rs`.
It collects six flags plus a positional directory, optionally drives an
interactive `inquire`-based wizard, and then calls
`libaipm::workspace_init::init` with a fixed `Vec<Box<dyn ToolAdaptor>>`
returned by `libaipm::workspace_init::adaptors::defaults()`. That `defaults()`
list contains exactly one element today — the Claude Code adaptor — so every
successful `aipm init` that creates a marketplace will also write or merge
`.claude/settings.json`. There is no flag on `Init` for selecting an engine,
opting out of Claude, or otherwise filtering which adaptors run.

## Entry Points

- `crates/aipm/src/main.rs:18-31` — top-level `Cli` parser (`#[derive(Parser)]`).
- `crates/aipm/src/main.rs:33-238` — `enum Commands` (clap `#[derive(Subcommand)]`).
- `crates/aipm/src/main.rs:35-64` — the `Init { … }` variant declaring the flags.
- `crates/aipm/src/main.rs:1200-1204` — dispatch arm in `run()` that constructs
  `InitWizardFlags` and calls `cmd_init`.
- `crates/aipm/src/main.rs:1281-1288` — `fn main()` thin wrapper around `run()`.

## Init command struct (verbatim)

From `crates/aipm/src/main.rs:35-64`:

```rust
/// Initialize a workspace for AI plugin management.
Init {
    /// Skip interactive prompts, use all defaults.
    #[arg(short = 'y', long)]
    yes: bool,

    /// Generate a workspace manifest (aipm.toml with [workspace] section).
    #[arg(long)]
    workspace: bool,

    /// Generate a .ai/ local marketplace with tool settings.
    #[arg(long)]
    marketplace: bool,

    /// Skip the starter plugin (create bare .ai/ directory only).
    #[arg(long)]
    no_starter: bool,

    /// Generate aipm.toml plugin manifests (opt-in; dependency management not yet available).
    #[arg(long)]
    manifest: bool,

    /// Custom marketplace name (default: "local-repo-plugins").
    #[arg(long)]
    name: Option<String>,

    /// Directory to initialize (defaults to current directory).
    #[arg(default_value = ".")]
    dir: PathBuf,
},
```

Flag inventory:

| Flag                | Type             | Default | Purpose                                                       |
|---------------------|------------------|---------|---------------------------------------------------------------|
| `-y` / `--yes`      | `bool`           | `false` | Skip wizard prompts; non-interactive defaulting               |
| `--workspace`       | `bool`           | `false` | Emit a workspace `aipm.toml` (`[workspace]` section)          |
| `--marketplace`     | `bool`           | `false` | Scaffold `.ai/` local marketplace + run tool adaptors         |
| `--no-starter`      | `bool`           | `false` | Skip the starter plugin (create bare `.ai/` only)             |
| `--manifest`        | `bool`           | `false` | Also emit per-plugin `aipm.toml` manifests                    |
| `--name <NAME>`     | `Option<String>` | `None`  | Override marketplace name (default `local-repo-plugins`)      |
| `dir` (positional)  | `PathBuf`        | `"."`   | Target directory                                              |

There are no subcommands under `Init` (note: the unrelated `aipm pack init`
subcommand at `main.rs:241-260` is a different code path that scaffolds a
single plugin package via `libaipm::init`).

## Call graph from CLI invocation to scaffolding

1. `fn main()` — `crates/aipm/src/main.rs:1281` — calls `run()`.
2. `fn run()` — invokes `Cli::parse()` and matches on `Commands::Init { … }` at
   `crates/aipm/src/main.rs:1201-1204`. It packs four of the booleans into
   `InitWizardFlags` (defined at `main.rs:390-396`) and forwards `manifest`,
   `name`, `dir` separately to `cmd_init`.
3. `fn cmd_init` — `crates/aipm/src/main.rs:398-445`:
   - `main.rs:404`: normalizes the dir via `resolve_dir(dir)` (`main.rs:320-326`,
     which expands `"."` to `std::env::current_dir()`).
   - `main.rs:405`: computes `interactive = !flags.yes && std::io::stdin().is_terminal()`.
   - `main.rs:407-411`: calls `wizard_tty::resolve(interactive, (workspace, marketplace, no_starter), name)` to obtain a
     `(bool, bool, bool, String)` tuple `(do_workspace, do_marketplace, do_no_starter, marketplace_name)`.
   - `main.rs:413`: gets the adaptor list from
     `libaipm::workspace_init::adaptors::defaults()`.
   - `main.rs:414-422`: builds `libaipm::workspace_init::Options { dir, workspace, marketplace, no_starter, manifest, marketplace_name }`.
   - `main.rs:423`: calls `libaipm::workspace_init::init(&opts, &adaptors, &libaipm::fs::Real)`.
   - `main.rs:425-443`: prints one human-readable line per
     `InitAction` returned (`WorkspaceCreated`, `MarketplaceCreated`,
     `ToolConfigured(name)`).
4. `libaipm::workspace_init::init` — `crates/libaipm/src/workspace_init/mod.rs:96-120`:
   - If `opts.workspace`, calls `init_workspace(opts.dir, fs)` and pushes `InitAction::WorkspaceCreated`.
   - If `opts.marketplace`, calls `scaffold_marketplace(opts.dir, opts.no_starter, opts.manifest, opts.marketplace_name, fs)`,
     pushes `InitAction::MarketplaceCreated`, and **iterates over every adaptor**
     (`for adaptor in adaptors`) calling `adaptor.apply(opts.dir, opts.no_starter, opts.marketplace_name, fs)`.
     If `apply` returns `true`, it pushes `InitAction::ToolConfigured(adaptor.name().to_string())`.
5. `libaipm::workspace_init::adaptors::defaults` — `crates/libaipm/src/workspace_init/adaptors/mod.rs:13-15`:

   ```rust
   pub fn defaults() -> Vec<Box<dyn ToolAdaptor>> {
       vec![Box::new(claude::Adaptor)]
   }
   ```

   The `Vec` is hard-coded to a single `claude::Adaptor`. There is no
   environment variable, config file, CLI flag, or runtime check that varies
   this list.
6. `claude::Adaptor::apply` — `crates/libaipm/src/workspace_init/adaptors/claude.rs:14-70`:
   - `claude.rs:26-29`: builds `dir.join(".claude")` and `dir.join(".claude/settings.json")`,
     then unconditionally `fs.create_dir_all(&settings_dir)`. **This is the call
     that creates `.claude/` even when no Claude user is present.**
   - `claude.rs:33-51`: reads or initializes `settings.json` as a JSON object.
   - `claude.rs:53-61`: merges `extraKnownMarketplaces.<marketplace_name>` and,
     unless `no_starter`, sets `enabledPlugins["starter-aipm-plugin@<name>"] = true`.
   - `claude.rs:63-68`: writes `settings.json` if anything changed.

## TTY vs non-TTY detection

- The check itself: `crates/aipm/src/main.rs:405` —
  `let interactive = !flags.yes && std::io::stdin().is_terminal();`
  (`std::io::IsTerminal` is imported at `main.rs:11`).
- Both conditions must hold for the wizard to run. Either passing `-y` /
  `--yes` or running with stdin redirected (CI, pipes) yields `interactive = false`.
- The same pattern appears in three other handlers (`main.rs:1027`, `:1065`,
  `:1170`) — `cmd_make_plugin`, the global-uninstall confirm path, and
  `cmd_pack_init`.

### Non-interactive fallback

When `interactive == false`, `wizard_tty::resolve` (`crates/aipm/src/wizard_tty.rs:48-50`) calls `wizard::resolve_defaults`
(`crates/aipm/src/wizard.rs:140-150`):

```rust
pub fn resolve_defaults(
    workspace: bool,
    marketplace: bool,
    no_starter: bool,
    name: Option<&str>,
) -> (bool, bool, bool, String) {
    let (w, m) = if !workspace && !marketplace { (false, true) } else { (workspace, marketplace) };
    let marketplace_name =
        name.filter(|s| !s.is_empty()).unwrap_or("local-repo-plugins").to_string();
    (w, m, no_starter, marketplace_name)
}
```

So `aipm init -y` (or `aipm init` in CI) with no other flags yields:
`workspace=false, marketplace=true, no_starter=false, marketplace_name="local-repo-plugins"`.
This means the marketplace branch in `init()` runs, which means
`claude::Adaptor::apply` runs, which means `.claude/settings.json` is created.

### Interactive (TTY) path

When `interactive == true`, `wizard_tty::resolve` (`crates/aipm/src/wizard_tty.rs:43-50`):

1. Calls `inquire::set_global_render_config(styled_render_config())` to apply theming.
2. Builds prompt steps via `wizard::workspace_prompt_steps(workspace, marketplace, no_starter, flag_name)`
   (`crates/aipm/src/wizard.rs:23-73`). The prompts are:
   - **Setup mode** (`Select`, `wizard.rs:36-40`) with three options
     (`SETUP_OPTIONS` at `wizard.rs:17-18`):
     `"Marketplace only (recommended)"`, `"Workspace manifest only"`,
     `"Both workspace + marketplace"`. Shown only when neither `--workspace`
     nor `--marketplace` was passed.
   - **Marketplace name** (`Text` with placeholder `local-repo-plugins`) —
     shown when marketplace is possible and `--name` was not given.
   - **Include starter plugin?** (`Confirm`, default true) — shown when
     marketplace is possible and `--no-starter` was not given.
3. Executes the prompts with `libaipm::wizard::execute_prompts(&steps)`
   (`crates/libaipm/src/wizard.rs:78-` — feature-gated behind the `wizard`
   Cargo feature, which the `aipm` binary enables; the `inquire` crate
   provides Select/Confirm/Text/MultiSelect prompts).
4. Maps answers back via `wizard::resolve_workspace_answers`
   (`crates/aipm/src/wizard.rs:78-131`) and returns the same
   `(bool, bool, bool, String)` tuple.

### Why `wizard_tty.rs` is excluded from coverage

`crates/aipm/src/wizard_tty.rs:1-9` documents that the file is a thin bridge
that only calls `inquire::*::prompt()` (via `libaipm::wizard::execute_prompts`)
and is excluded from the coverage gate because it requires a real TTY. All
pure logic (prompt definitions, answer resolution, theming) lives in
`crates/aipm/src/wizard.rs` and is fully tested. The coverage-ignore regex
in `CLAUDE.md` lists `wizard_tty\.rs` among the excluded paths.

## Engine-selection flags on `Init`: NONE

Confirmed by reading the entire `Init { … }` declaration at
`crates/aipm/src/main.rs:35-64` and the dispatch arm at `:1201-1204`:

- There is **no `--engine` flag** on `Init`.
- There is **no `--engines` flag** on `Init`.
- There is **no `--no-claude` flag** on `Init`.
- There is **no opt-out** for the Claude adaptor from `init` callers.

Engine flags exist on **other** commands only:

- `Install { … --engine: Option<String> … }` — `main.rs:83-85`
- `Uninstall { … --engine: Option<String> … }` — `main.rs:125-127`
- `MakeSubcommand::Plugin { … --engine: Option<String> … }` — `main.rs:270-272`

The string `"claude"` appears as a hard-coded default in `make plugin`
(`crates/aipm/src/wizard_tty.rs:98, :139`) and as the sole entry of
`starter_engines` in the generated starter manifest
(`crates/libaipm/src/workspace_init/mod.rs:282`). The starter scaffold script
itself defaults to claude:
`aipm make plugin --name "..." --engine "${2:-claude}" --feature skill -y`
(`crates/libaipm/src/workspace_init/mod.rs:334`).

## Data flow summary

1. User types `aipm init [flags] [dir]`.
2. clap parses into `Commands::Init { yes, workspace, marketplace, no_starter, manifest, name, dir }` (`main.rs:36`, dispatched at `:1201`).
3. `cmd_init` resolves dir, computes `interactive`, runs
   `wizard_tty::resolve(...)` (`main.rs:407`).
4. `wizard_tty::resolve` either runs the interactive wizard or applies
   `resolve_defaults` (TTY branch picked by `is_terminal()`).
5. `cmd_init` builds `libaipm::workspace_init::Options` and calls
   `libaipm::workspace_init::init(&opts, &adaptors, &libaipm::fs::Real)`
   with `adaptors = libaipm::workspace_init::adaptors::defaults() == [claude::Adaptor]`.
6. `init()` may write the workspace manifest, may scaffold `.ai/`, then
   iterates the adaptor list — Claude's `apply` always runs `.claude/`
   creation when `marketplace == true`.
7. `cmd_init` prints one line per `InitAction`.

## Key file references

- `/workspaces/aipm/crates/aipm/src/main.rs:11` — `use std::io::{IsTerminal, Write};`
- `/workspaces/aipm/crates/aipm/src/main.rs:18-31` — `Cli` struct
- `/workspaces/aipm/crates/aipm/src/main.rs:33-238` — `Commands` enum
- `/workspaces/aipm/crates/aipm/src/main.rs:35-64` — `Init` variant + flags
- `/workspaces/aipm/crates/aipm/src/main.rs:241-260` — unrelated `PackSubcommand::Init`
- `/workspaces/aipm/crates/aipm/src/main.rs:320-326` — `resolve_dir`
- `/workspaces/aipm/crates/aipm/src/main.rs:390-396` — `InitWizardFlags`
- `/workspaces/aipm/crates/aipm/src/main.rs:398-445` — `cmd_init`
- `/workspaces/aipm/crates/aipm/src/main.rs:1201-1204` — Init dispatch arm
- `/workspaces/aipm/crates/aipm/src/main.rs:1281-1288` — `main`
- `/workspaces/aipm/crates/aipm/src/wizard_tty.rs:37-51` — `resolve` (init TTY bridge)
- `/workspaces/aipm/crates/aipm/src/wizard.rs:17-18` — `SETUP_OPTIONS`
- `/workspaces/aipm/crates/aipm/src/wizard.rs:23-73` — `workspace_prompt_steps`
- `/workspaces/aipm/crates/aipm/src/wizard.rs:78-131` — `resolve_workspace_answers`
- `/workspaces/aipm/crates/aipm/src/wizard.rs:140-150` — `resolve_defaults`
- `/workspaces/aipm/crates/libaipm/src/wizard.rs:78-` — `execute_prompts` (inquire bridge)
- `/workspaces/aipm/crates/libaipm/src/workspace_init/mod.rs:7-15` — module exports + `Error`/`adaptors` re-exports
- `/workspaces/aipm/crates/libaipm/src/workspace_init/mod.rs:20-49` — `ToolAdaptor` trait
- `/workspaces/aipm/crates/libaipm/src/workspace_init/mod.rs:51-65` — `Options` struct
- `/workspaces/aipm/crates/libaipm/src/workspace_init/mod.rs:67-79` — `InitAction`
- `/workspaces/aipm/crates/libaipm/src/workspace_init/mod.rs:96-120` — `init()` (adaptor loop at 112-116)
- `/workspaces/aipm/crates/libaipm/src/workspace_init/mod.rs:174-274` — `scaffold_marketplace`
- `/workspaces/aipm/crates/libaipm/src/workspace_init/adaptors/mod.rs:13-15` — `defaults()` (hard-coded `[claude::Adaptor]`)
- `/workspaces/aipm/crates/libaipm/src/workspace_init/adaptors/claude.rs:14-70` — `claude::Adaptor::apply` (creates `.claude/`)
- `/workspaces/aipm/CLAUDE.md` — coverage-ignore list including `wizard_tty\.rs`
