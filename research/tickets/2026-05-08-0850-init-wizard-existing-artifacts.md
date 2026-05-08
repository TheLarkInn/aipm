---
date: 2026-05-08 21:07:47 UTC
researcher: Sean Larkin
git_commit: 2dfb73eb6dd44ac514dcd107cee97854fd674eb6
branch: main
repository: aipm
topic: "[init] If something already exists in the wizard don't fail (#850)"
tags: [research, codebase, init, wizard, workspace-init, scaffolding, idempotency, marketplace, aipm-toml, tracing]
status: complete
last_updated: 2026-05-08
last_updated_by: Sean Larkin
---

# Research

## Research Question

GitHub issue [#850](https://github.com/TheLarkInn/aipm/issues/850):

> - If `.ai` folder exists the wizard fails, we shouldn't do that, lets use and add to the .ai folder on `aipm init`
>   - If there is a `.claude-plugins/marketplace.json` folder in the .ai folder, then also don't error for `aipm init`
>   - When `.ai` is already found, log that `.ai` is found (info level)
>   - Log that the `marketplace.json` file is found (info level)
> - If `aipm.toml` config file already exists, don't just fail the wizard for `aipm init`, if the file exists, write a log which says "using existing `aipm.toml` file" (info level).

The research question: **document the current code paths through which `aipm init` rejects pre-existing `.ai/`, `aipm.toml`, and `.ai/.claude-plugin/marketplace.json` artifacts, the logging facility the codebase uses for info-level messages, and prior research/specs that touch the init wizard or its idempotency.** Pure documentation — no recommendations.

## Summary

`aipm init` runs an `inquire`-driven wizard in `crates/aipm/src/`, builds a `libaipm::workspace_init::Options` value, and calls the library entry point `libaipm::workspace_init::init`. **The wizard layer itself performs no existence checks** — every existence guard lives inside `libaipm::workspace_init`.

There are **two existence guards** today and **one implicit guard** masquerading as a third:

| # | Artifact | Detection site | Error variant | Behavior |
|---|---|---|---|---|
| 1 | `aipm.toml` (workspace root) | `init_workspace` at `crates/libaipm/src/workspace_init/mod.rs:162-165` | `Error::WorkspaceAlreadyInitialized(PathBuf)` | Hard fail. Triggered only when `opts.workspace == true`. |
| 2 | `.ai/` directory | `scaffold_marketplace` at `crates/libaipm/src/workspace_init/mod.rs:249-252` | `Error::MarketplaceAlreadyExists(PathBuf)` | Hard fail. Triggered only when `opts.marketplace == true`. |
| 3 | `.ai/.claude-plugin/marketplace.json` | **No dedicated check** — guard #2 fires first because `.ai/` is the parent directory and is rejected unconditionally. | n/a | Once `.ai/` exists, the marketplace-json write at `crates/libaipm/src/workspace_init/mod.rs:269-282` is unreachable. |

Both errors propagate via `?` through `cmd_init` (`crates/aipm/src/main.rs:443`) and surface as `CliError::WorkspaceInit` (`crates/aipm/src/error.rs:14-16`), which is `#[error(transparent)]`, so the user sees the raw `thiserror` `Display` strings:

- `already initialized: aipm.toml already exists in <dir>`
- `.ai/ marketplace already exists in <dir>`

The codebase uses **`tracing`** as its sole logging crate (workspace `Cargo.toml`). The canonical info-level idiom is fully-qualified `tracing::info!(field = %value, "short verb-phrase")`, used 21 times across the workspace, e.g. `tracing::info!(manifest = %config.manifest_path.display(), "loading manifest")` at `crates/libaipm/src/installer/pipeline.rs:82`. **No wrapper helper exists.** User-facing CLI output is a separate channel — `writeln!(stdout, …)` from `crates/aipm/src/main.rs` and `writeln!(stderr, …)` for the wizard summary in `crates/aipm/src/wizard_tty.rs:67-74`. The lint policy in `CLAUDE.md` forbids `println!`/`eprintln!`.

Two unit tests in `crates/libaipm/src/workspace_init/mod.rs` currently encode the "fail on already-exists" contract: `init_workspace_rejects_existing` (line 583) and `init_marketplace_rejects_existing` (line 607). Any change to make the init idempotent must update or replace these.

There is **no existing research ticket for #850** and **no entry in `research/feature-list.json`/`research/progress.txt`** — those files currently track only PR #793's security work.

## Detailed Findings

### Init command call chain

The CLI dispatches `Init` in `crates/aipm/src/main.rs:1222-1234`:

```text
clap::Commands::Init { … }
    └─► cmd_init(flags, manifest, name, dir)               main.rs:405-466
            ├─► resolve_dir(dir)
            ├─► wizard_tty::resolve(interactive, …)        wizard_tty.rs:40-81
            │     ├─► (interactive) inquire::*.prompt()
            │     └─► (non-interactive) flag-derived defaults via wizard.rs
            ├─► libaipm::workspace_init::adaptors::defaults()
            ├─► libaipm::workspace_init::init(&opts, …)    workspace_init/mod.rs:115-151
            │     ├─► [opts.workspace]   init_workspace(…)         line 122  → mod.rs:157-177
            │     ├─► [opts.marketplace] scaffold_marketplace(…)   line 127  → mod.rs:241-342
            │     └─► [for each adaptor] adaptor.apply(…)          line 138  → adaptors/{claude,copilot}.rs
            └─► writeln!(stdout, "{msg}") for each InitAction      main.rs:445-464
```

The wizard layer (`crates/aipm/src/wizard.rs`, `wizard_tty.rs`) builds the `Options` from prompt answers but **does not** look at the filesystem. All gating is in `libaipm::workspace_init`.

### Failure path 1 — pre-existing `aipm.toml`

**Detection:** `crates/libaipm/src/workspace_init/mod.rs:162-165`

```rust
let manifest_path = dir.join("aipm.toml");
if fs.exists(&manifest_path) {
    return Err(Error::WorkspaceAlreadyInitialized(dir.to_path_buf()));
}
```

**Function:** `init_workspace(dir, engines_support, fs)` — `crates/libaipm/src/workspace_init/mod.rs:157-177`. It runs only when the wizard's `Options.workspace == true` (line 122).

**Error variant:** `Error::WorkspaceAlreadyInitialized(PathBuf)` — `crates/libaipm/src/workspace_init/error.rs:8-10`:

```rust
#[error("already initialized: aipm.toml already exists in {}", .0.display())]
WorkspaceAlreadyInitialized(PathBuf),
```

**Bypasses today:**
- `opts.workspace == false` (e.g. `aipm init --marketplace` without `--workspace`) — `init_workspace` is never called.
- No content-aware short-circuit. The check does not look at the existing manifest's content.

**Test asserting the contract:** `crates/libaipm/src/workspace_init/mod.rs:582-604` (`init_workspace_rejects_existing`).

**Surfacing:** `Error` flows through `cmd_init`'s `?` at `main.rs:443`, into `CliError::WorkspaceInit(#[from] libaipm::workspace_init::Error)` at `crates/aipm/src/error.rs:14-16`. The `#[error(transparent)]` attribute means the user sees the raw `Display` string verbatim.

**Unrelated lookalike:** `crates/libaipm/src/init.rs:62-65` defines a separate `Error::AlreadyInitialized` for the **`aipm pack init`** flow (plugin-package scaffolding via `cmd_pack_init` at `main.rs:1182-1204`). That is a different command and is not part of issue #850.

### Failure path 2 — pre-existing `.ai/` directory

**Detection:** `crates/libaipm/src/workspace_init/mod.rs:249-252`

```rust
let ai_dir = dir.join(".ai");
if fs.exists(&ai_dir) {
    return Err(Error::MarketplaceAlreadyExists(dir.to_path_buf()));
}
```

**Function:** `scaffold_marketplace(dir, no_starter, manifest, marketplace_name, engines_support, fs)` — `crates/libaipm/src/workspace_init/mod.rs:241-342`. Runs only when `Options.marketplace == true` (line 127).

**Error variant:** `Error::MarketplaceAlreadyExists(PathBuf)` — `crates/libaipm/src/workspace_init/error.rs:12-14`:

```rust
#[error(".ai/ marketplace already exists in {}", .0.display())]
MarketplaceAlreadyExists(PathBuf),
```

**Bypasses today:**
- `opts.marketplace == false` — `scaffold_marketplace` is never called.
- No content-aware short-circuit. `fs.exists(&ai_dir)` returns `true` for an empty `.ai/` as well as a populated one. No name/version/marker comparison.

**Effect on adaptors:** Tool adaptors (Claude, Copilot) at `mod.rs:138-147` only run if `scaffold_marketplace` returned `Ok(())`. When `.ai/` exists, no adaptor side effects occur — even if the user expected `.claude/settings.json` or `.github/copilot-instructions.md` updates.

**Test asserting the contract:** `crates/libaipm/src/workspace_init/mod.rs:606-628` (`init_marketplace_rejects_existing`).

### Failure path 3 — pre-existing `.ai/.claude-plugin/marketplace.json`

**No dedicated detection site exists.** The `.ai/` parent guard (path 2) fires first whenever `.ai/` is present. When `.ai/` does **not** exist, `scaffold_marketplace` proceeds unconditionally to:

1. `fs.create_dir_all(&ai_dir)` — `mod.rs:255`
2. write `.ai/.gitignore` — `mod.rs:267`
3. `fs.create_dir_all(&ai_dir.join(".claude-plugin"))` — `mod.rs:270`
4. `fs.write_file(&ai_dir.join(".claude-plugin").join("marketplace.json"), …)` — `mod.rs:279-282`

There is no `fs.exists` check around step 4, no parsing of an existing manifest, no merging with prior plugin entries.

**Path constants:** defined at build time in `crates/libaipm-engine-spec/build.rs:369-371` (`CLAUDE_PLUGIN_DIR = ".claude-plugin"`, `MARKETPLACE_JSON = "marketplace.json"`). The `engine.rs` helper `marketplace_manifest_path(Engine)` at `crates/libaipm/src/engine.rs` maps engine variants to `.claude-plugin/marketplace.json` vs `.github/plugin/marketplace.json`.

**Note:** The issue text says "if there is a `.claude-plugins/marketplace.json` *folder* in the .ai folder" (with an `s` and called "folder"). The actual on-disk path written by init is `.ai/.claude-plugin/marketplace.json` (singular, file). The reverse-binary-analysis-derived constant `CLAUDE_PLUGIN_DIR = ".claude-plugin"` is the source of truth.

### Adaptor-level idempotency (related precedent, different artifacts)

Two adaptors already implement non-fatal "skip-if-present" behaviour. These set the precedent for what idempotency could look like, though they apply to engine-side files rather than `.ai/`:

- **Copilot adaptor** — `crates/libaipm/src/workspace_init/adaptors/copilot.rs:40-42`: if `.github/copilot-instructions.md` exists, `apply` returns `Ok(false)` (the boolean signalling "no `ToolConfigured` action recorded"). No log emitted today.
- **Claude adaptor** — `crates/libaipm/src/workspace_init/adaptors/claude.rs:39-46`: `.claude/settings.json` is read with `fs.read_to_string`; `ErrorKind::NotFound` defaults to an empty JSON object so the adaptor merges into an existing file in-place. A parse failure (existing-but-malformed JSON) returns `Error::JsonParse { path, source }` (`error.rs:21-27`).

The `apply` trait signature already returns `Result<bool, Error>`, where `false` signals a no-op skip (`workspace_init/mod.rs:144-146`).

### Logging facility

**Crate:** `tracing` (with `tracing-subscriber` and `tracing-appender`). Declared in workspace `Cargo.toml`:

- `tracing = "0.1"`
- `tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }`
- `tracing-appender = "0.2"`
- `clap-verbosity-flag = { version = "3", default-features = false, features = ["tracing"] }`

**Initialisation:** `libaipm::logging::init(verbosity, log_fmt)?` at `crates/aipm/src/main.rs:1219`, implementation at `crates/libaipm/src/logging.rs:67-112`. Two layers:
- **stderr layer** — verbosity-controlled, overridable via `AIPM_LOG`, supports text or JSON format.
- **file layer** — always-on at `DEBUG`, daily rotation, 7-day retention, written to `<temp_dir>/aipm-YYYY-MM-DD.log`.

**Canonical idiom** (21 call sites total, all fully-qualified, no `use tracing::info`):

```rust
tracing::info!(manifest = %config.manifest_path.display(), "loading manifest");
// crates/libaipm/src/installer/pipeline.rs:82

tracing::info!(url = %self.index_url, dir = %self.cache_dir.display(), "cloning registry index");
// crates/libaipm/src/registry/git.rs:83

tracing::info!(workspace_root = %ws_root.display(), "found workspace root");
// crates/aipm/src/main.rs:475
```

Multi-line form (used when there are several structured fields):

```rust
tracing::info!(
    installed = installed,
    up_to_date = up_to_date,
    removed = removed,
    "install complete"
);
// crates/libaipm/src/installer/pipeline.rs:202-207
```

The `%` sigil invokes `Display`; `?` invokes `Debug`. Message strings are short verb phrases at the end of the macro call.

**No wrapper helper.** Searches for `cli::info`, `fn cli_info`, `fn user_print`, `fn ui_info`, `fn say`, `fn emit` returned no results. Every info-level call invokes `tracing::info!` directly.

**Two distinct output channels:**

| Channel | Used for | Example |
|---|---|---|
| `writeln!(stdout, …)` | User-facing CLI output (command results) | `crates/aipm/src/main.rs:463`, `:498-502`, `:573`, `:593`, `:625-629`, `:992` |
| `writeln!(stderr, …)` | Wizard confirmation summary | `crates/aipm/src/wizard_tty.rs:67-74` (with explicit comment about the no-`println!` lint) |
| `tracing::{info,debug,warn,trace}!` | Diagnostic / progress events from inside `crates/libaipm/` | `crates/libaipm/src/installer/pipeline.rs:82`, etc. |

The CLI binary (`crates/aipm/`) has only **one** `tracing::info!` call (`main.rs:475`); the rest of its output is `writeln!(stdout, …)`. Library code never writes to stdout — it emits via `tracing`.

### Tests gating current behaviour

#### Unit tests (`crates/libaipm/src/workspace_init/mod.rs`)
- `init_workspace_rejects_existing` (line 582-604) — creates `aipm.toml`, asserts `init` returns an error whose `Display` contains `"already initialized"`.
- `init_marketplace_rejects_existing` (line 606-628) — creates `.ai/`, asserts `init` returns an error whose `Display` contains `"already exists"`.
- `init_both_creates_everything` (line 630+) — happy path where neither artifact pre-exists.
- Adaptor-specific tests in `adaptors/copilot.rs` and `adaptors/claude.rs` for their idempotent paths.

#### CLI E2E (`crates/aipm/tests/`)
- `init_e2e.rs` — covers `aipm init`, `--workspace`, `--marketplace`, `--no-starter`, default flag combinations.
- `init_engine_e2e.rs` — engine-aware init scenarios.
- `pack_init_e2e.rs` — `aipm pack init` plugin-package flow (separate from #850).
- `migrate_e2e.rs`, `lint_e2e.rs`, `issue_725_e2e.rs` — use `aipm init` as setup but do not exercise the existence-fail paths directly.

#### BDD (`tests/features/`)
- `tests/features/manifest/workspace-init.feature` — primary workspace-init Gherkin spec.
- `tests/features/manifest/init.feature` — `aipm pack init` scenarios.
- `tests/features/manifest/migrate.feature`, `registry/marketplace.feature`, `portability/cross-stack.feature`, `monorepo/orchestration.feature`, `guardrails/quality.feature` — touch init in setup.
- BDD steps live in `crates/libaipm/tests/bdd.rs:459-710`; init-related step helpers (`given_initialized_marketplace`, `given_workspace_initialized`) shell out to `aipm init`.

## Code References

### Files that change behaviour
- [`crates/libaipm/src/workspace_init/mod.rs:115-151`](https://github.com/TheLarkInn/aipm/blob/2dfb73eb6dd44ac514dcd107cee97854fd674eb6/crates/libaipm/src/workspace_init/mod.rs#L115-L151) — `init()` orchestrator.
- [`crates/libaipm/src/workspace_init/mod.rs:157-177`](https://github.com/TheLarkInn/aipm/blob/2dfb73eb6dd44ac514dcd107cee97854fd674eb6/crates/libaipm/src/workspace_init/mod.rs#L157-L177) — `init_workspace`, the `aipm.toml` guard.
- [`crates/libaipm/src/workspace_init/mod.rs:241-342`](https://github.com/TheLarkInn/aipm/blob/2dfb73eb6dd44ac514dcd107cee97854fd674eb6/crates/libaipm/src/workspace_init/mod.rs#L241-L342) — `scaffold_marketplace`, the `.ai/` guard and unconditional `marketplace.json` write.
- [`crates/libaipm/src/workspace_init/error.rs:1-29`](https://github.com/TheLarkInn/aipm/blob/2dfb73eb6dd44ac514dcd107cee97854fd674eb6/crates/libaipm/src/workspace_init/error.rs#L1-L29) — `Error` enum (`WorkspaceAlreadyInitialized`, `MarketplaceAlreadyExists`, `Io`, `JsonParse`).
- [`crates/libaipm/src/workspace_init/adaptors/claude.rs:39-46`](https://github.com/TheLarkInn/aipm/blob/2dfb73eb6dd44ac514dcd107cee97854fd674eb6/crates/libaipm/src/workspace_init/adaptors/claude.rs#L39-L46) — Claude adaptor merge-into-existing-settings precedent.
- [`crates/libaipm/src/workspace_init/adaptors/copilot.rs:27-48`](https://github.com/TheLarkInn/aipm/blob/2dfb73eb6dd44ac514dcd107cee97854fd674eb6/crates/libaipm/src/workspace_init/adaptors/copilot.rs#L27-L48) — Copilot adaptor `Ok(false)` skip-when-exists precedent.

### CLI surface
- [`crates/aipm/src/main.rs:405-466`](https://github.com/TheLarkInn/aipm/blob/2dfb73eb6dd44ac514dcd107cee97854fd674eb6/crates/aipm/src/main.rs#L405-L466) — `cmd_init`.
- [`crates/aipm/src/main.rs:1222-1234`](https://github.com/TheLarkInn/aipm/blob/2dfb73eb6dd44ac514dcd107cee97854fd674eb6/crates/aipm/src/main.rs#L1222-L1234) — `Commands::Init` dispatch.
- [`crates/aipm/src/main.rs:1213-1219`](https://github.com/TheLarkInn/aipm/blob/2dfb73eb6dd44ac514dcd107cee97854fd674eb6/crates/aipm/src/main.rs#L1213-L1219) — `logging::init` call.
- [`crates/aipm/src/error.rs:14-16`](https://github.com/TheLarkInn/aipm/blob/2dfb73eb6dd44ac514dcd107cee97854fd674eb6/crates/aipm/src/error.rs#L14-L16) — `CliError::WorkspaceInit` (`#[from]`, transparent).
- [`crates/aipm/src/wizard.rs`](https://github.com/TheLarkInn/aipm/blob/2dfb73eb6dd44ac514dcd107cee97854fd674eb6/crates/aipm/src/wizard.rs) — wizard prompt definitions, `format_wizard_summary`, no existence checks.
- [`crates/aipm/src/wizard_tty.rs:40-81`](https://github.com/TheLarkInn/aipm/blob/2dfb73eb6dd44ac514dcd107cee97854fd674eb6/crates/aipm/src/wizard_tty.rs#L40-L81) — `resolve()` TTY bridge; writes summary to stderr (lines 67-74).

### Logging
- [`crates/libaipm/src/logging.rs:67-112`](https://github.com/TheLarkInn/aipm/blob/2dfb73eb6dd44ac514dcd107cee97854fd674eb6/crates/libaipm/src/logging.rs#L67-L112) — subscriber setup.
- [`crates/libaipm/src/installer/pipeline.rs:82`](https://github.com/TheLarkInn/aipm/blob/2dfb73eb6dd44ac514dcd107cee97854fd674eb6/crates/libaipm/src/installer/pipeline.rs#L82) — canonical info-level idiom.
- [`crates/libaipm/src/registry/git.rs:83`](https://github.com/TheLarkInn/aipm/blob/2dfb73eb6dd44ac514dcd107cee97854fd674eb6/crates/libaipm/src/registry/git.rs#L83) — multi-field info call.

### Tests
- [`crates/libaipm/src/workspace_init/mod.rs:582-604`](https://github.com/TheLarkInn/aipm/blob/2dfb73eb6dd44ac514dcd107cee97854fd674eb6/crates/libaipm/src/workspace_init/mod.rs#L582-L604) — `init_workspace_rejects_existing`.
- [`crates/libaipm/src/workspace_init/mod.rs:606-628`](https://github.com/TheLarkInn/aipm/blob/2dfb73eb6dd44ac514dcd107cee97854fd674eb6/crates/libaipm/src/workspace_init/mod.rs#L606-L628) — `init_marketplace_rejects_existing`.
- [`tests/features/manifest/workspace-init.feature`](https://github.com/TheLarkInn/aipm/blob/2dfb73eb6dd44ac514dcd107cee97854fd674eb6/tests/features/manifest/workspace-init.feature) — Gherkin spec.
- [`crates/aipm/tests/init_e2e.rs`](https://github.com/TheLarkInn/aipm/blob/2dfb73eb6dd44ac514dcd107cee97854fd674eb6/crates/aipm/tests/init_e2e.rs) — CLI E2E for init.

### Path constants
- [`crates/libaipm-engine-spec/build.rs:369-371`](https://github.com/TheLarkInn/aipm/blob/2dfb73eb6dd44ac514dcd107cee97854fd674eb6/crates/libaipm-engine-spec/build.rs#L369-L371) — `AIPM_TOML`, `MARKETPLACE_JSON`, `CLAUDE_PLUGIN_DIR`.

## Architecture Documentation

### Layered structure
The init flow follows a clean three-layer split:

1. **CLI binary (`crates/aipm/`)** — clap dispatch, TTY detection, wizard orchestration, stdout output. Owns user-facing strings.
2. **Library (`crates/libaipm/workspace_init/`)** — pure business logic over an `Fs` trait abstraction. All filesystem effects flow through `crate::fs::Fs` (real or in-memory). All `Result::Err` returns are typed via `Error`.
3. **Adaptor trait (`workspace_init::adaptors::ToolAdaptor`)** — pluggable per-engine post-scaffold hooks (Claude writes `.claude/settings.json`, Copilot writes `.github/copilot-instructions.md`). Adaptors return `Result<bool, Error>`; `Ok(false)` already encodes a no-op skip.

### Where existence checks live and where they don't
All filesystem state inspection happens inside `libaipm::workspace_init`. The wizard layer is **pure** — it converts CLI flags + prompt answers into an `Options` struct and never calls into the filesystem. This means any change to make pre-existing artifacts non-fatal can stay inside `workspace_init/mod.rs` and `error.rs` without touching wizard prompt code or wizard snapshot tests (`crates/aipm/src/snapshots/`).

### Two output channels by convention
- **Library** → `tracing::{info,debug,warn,trace}!` only. Never writes to stdout.
- **CLI binary** → `writeln!(stdout, …)` for command results; `writeln!(stderr, …)` for the wizard confirmation block. `tracing::info!` is used once (`main.rs:475`) as a diagnostic, not as user output.

The CLAUDE.md lint policy denies `println!`/`eprintln!`/`print!`/`eprint!`, so any new informational output must use either `tracing::info!` (structured) or `writeln!` (CLI-facing).

### `Error` propagation contract
`workspace_init::Error` is mapped into `aipm::error::CliError::WorkspaceInit` via `#[from]` and surfaced with `#[error(transparent)]`. The user sees the `thiserror` `Display` string verbatim. Two of the four variants (`WorkspaceAlreadyInitialized`, `MarketplaceAlreadyExists`) are the ones #850 wants converted to non-fatal info-level logs.

## Historical Context (from research/)

**Most relevant to #850:**
- `research/docs/2026-05-05-init-cli-entry-point.md` — `Research: aipm init CLI entry point and call graph` (2026-05-05). Direct predecessor mapping `cmd_init` → `workspace_init::init`.
- `research/docs/2026-05-05-init-scaffolding-trace.md` — `aipm init Scaffolding Trace` (2026-05-05). Runtime trace of every file written across `.ai/`, `.claude-plugin/`, `.claude/`, `.github/`.
- `research/docs/2026-05-05-wizard-prompt-flow.md` — `aipm init Wizard Prompt Flow (current)` (2026-05-05). Documents the present-day prompt sequence (no existence-aware branches yet).
- `research/docs/2026-03-16-aipm-init-workspace-marketplace.md` — `Research: aipm init --workspace --marketplace` (2026-03-16). Original walkthrough of the `--workspace` / `--marketplace` flag matrix.
- `research/docs/2026-03-19-init-tool-adaptor-refactor.md` — refactor that introduced the `ToolAdaptor` trait and the `Ok(false)` skip-when-exists pattern.
- `research/docs/2026-03-22-rust-interactive-cli-prompts.md` — survey of `inquire`/`dialoguer`; explains why the wizard layer is pure.
- `research/docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md` — how the manifest is produced; touches overwrite/already-exists behaviour.
- `research/docs/2026-03-25-marketplace-name-customization-in-init.md` — adds the marketplace `name` prompt to the wizard.

**Specs of record:**
- `specs/2026-03-16-aipm-init-workspace-marketplace.md` — `aipm init — Workspace & Marketplace Scaffolding` (2026-03-16). The canonical spec for the two flags whose guards #850 targets.
- `specs/2026-03-19-init-tool-adaptor-refactor.md` — formalises the adaptor pattern.
- `specs/2026-03-22-interactive-init-wizard.md` — `Interactive Init Wizard with inquire` (2026-03-22). Source of the `wizard_tty.rs` § 5.2.4 "stderr summary" comment.
- `specs/2026-05-05-init-engine-aware-wizard.md` — `Engine-Aware aipm init Wizard — Technical Design Document / RFC` (2026-05-05). Latest in-flight RFC for the engine-selection prompts.
- `specs/2026-03-24-suppress-plugin-manifest-generation.md` — controls when init writes plugin manifests; example of an existence/skip behaviour that already shipped.

**Adjacent tickets:**
- `research/tickets/2026-05-05-0724-init-engine-aware-wizard.md` — engine-aware wizard (#724).
- `research/tickets/2026-05-01-510-aipm-toml-engines.md` — `aipm.toml` `engines` field (#510, #724, #697).
- `research/tickets/2026-04-14-0363-aipm-make-foundational-api.md` — atomic scaffolding primitives (#363); covers idempotency philosophy for `aipm make`.

**Tracking files:**
- `research/feature-list.json` and `research/progress.txt` are currently dedicated to PR #793's security workstream. **No entry references issue #850, "wizard idempotency", or "init existing".** A new feature entry would need to be added before this issue is implemented under the standard ralph/feature-list workflow.

## Related Research

- `research/tickets/2026-05-05-0724-init-engine-aware-wizard.md` — most architecturally adjacent active ticket; any change here interacts with the engine-aware prompt flow.
- `research/tickets/2026-04-14-0363-aipm-make-foundational-api.md` — `aipm make` primitives are explicitly designed to be idempotent; their patterns (atomic write + content-aware skip) are the closest internal precedent for what #850 asks for in `aipm init`.
- `research/docs/2026-03-19-init-tool-adaptor-refactor.md` — explains why `ToolAdaptor::apply` returns `Result<bool, Error>`; the same `Ok(false)` skip-signal could be applied to the workspace and marketplace pre-checks.

## Open Questions

These are observations the research surfaced — they are not recommendations, but they are the questions a downstream spec/implementation will need to resolve:

1. **Issue text vs. on-disk path mismatch.** The issue refers to a `.claude-plugins/marketplace.json` "folder" inside `.ai/`. The actual on-disk path is the file `.ai/.claude-plugin/marketplace.json` (singular, file). Implementer must reconcile.
2. **Semantics of "use existing `.ai/`".** Does it mean (a) leave `.ai/` and its starter plugin entirely untouched, (b) merge a new starter plugin entry into the existing `marketplace.json`, or (c) re-run only the per-engine adaptors (Claude/Copilot) over the existing tree?  The issue checklist suggests (a), but that re-introduces the question of whether the user gets any signal beyond the info log when their flags would have changed something.
3. **`opts.workspace == true` + existing `aipm.toml` semantics.** "Use existing" — is the existing manifest validated? Re-parsed? Compared to the wizard's intended new manifest? The issue says "log and continue" but does not address what `--engine`/`--workspace`/`--marketplace` flags do when the manifest already pins different values.
4. **Two existing tests must change.** `init_workspace_rejects_existing` (line 582) and `init_marketplace_rejects_existing` (line 607) currently encode "fail" as the contract. Any implementation must invert these (likely renaming to `init_workspace_idempotent` / `init_marketplace_idempotent`) and add coverage for the new info-log emission.
5. **Logging assertion strategy.** `tracing` events are not captured by default in `cargo test`. New tests asserting "info log was emitted" need a `tracing-subscriber::test` layer or `tracing-test` crate hookup that does not yet appear in `Cargo.toml` (the workspace declares `tracing-subscriber` with `env-filter,json` features only).
6. **Error variants `WorkspaceAlreadyInitialized` and `MarketplaceAlreadyExists`** become unreachable from the `aipm init` path if the change is purely make-idempotent. They may still be needed for `aipm pack init` (which uses a different `Error::AlreadyInitialized` in `init.rs`, not these variants). Whether to keep, deprecate, or repurpose them is a downstream decision.
