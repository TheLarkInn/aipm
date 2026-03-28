---
date: 2026-03-27 16:40:28 PDT
researcher: Claude
git_commit: b034f7a3c3326ea746e8afd8bee63a7170899ca4
branch: main
repository: aipm
topic: "aipm migrate --destructive flag and post-migration cleanup wizard"
tags: [research, codebase, migrate, destructive, wizard, cli-flags, cleanup]
status: complete
last_updated: 2026-03-27
last_updated_by: Claude
---

# Research: `aipm migrate --destructive` Flag (Issue #111)

## Research Question

GitHub Issue #111 requests:
1. An `aipm migrate --destructive` flag that removes old `.claude/` source files after migration
2. A wizard step after bare `aipm migrate` (without `--destructive`) asking if the user wants to remove the old copies (yes/no)
3. When `--destructive` is passed, the wizard step never triggers

## Summary

The current `aipm migrate` command is explicitly non-destructive: it **copies** artifacts from `.claude/` directories into `.ai/` plugin directories, leaving all original files intact. The codebase has well-established patterns for both CLI flags (via clap derive) and interactive wizard prompts (via the `inquire` crate with a two-layer architecture), as well as filesystem deletion operations (behind the `Fs` trait abstraction). All the building blocks needed to implement `--destructive` and its companion wizard step already exist in the codebase.

## Detailed Findings

### 1. Current `aipm migrate` Implementation

The migrate command is defined across two crates:

**CLI layer** ([`crates/aipm/src/main.rs:113-135`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/aipm/src/main.rs#L113-L135)):

Current flags on the `Migrate` variant:

| Flag | Type | Default | Purpose |
|------|------|---------|---------|
| `--dry-run` | `bool` | `false` | Preview migration without writing files |
| `--source` | `Option<String>` | `None` | Explicit source folder name (e.g., `".claude"`) |
| `--max-depth` | `Option<usize>` | `None` | Max traversal depth for recursive discovery |
| `--manifest` | `bool` | `false` | Generate `aipm.toml` plugin manifests |
| `dir` (positional) | `PathBuf` | `"."` | Project directory |

There is currently **no `--destructive`, `--force`, `--yes`, or `--clean` flag** on the `Migrate` subcommand.

**Library layer** ([`crates/libaipm/src/migrate/mod.rs:235-247`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/mod.rs#L235-L247)):

The `migrate()` entry point validates `.ai/` exists, then branches:
- If `--source` is set: calls `migrate_single_source()` ([`mod.rs:250-312`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/mod.rs#L250-L312))
- Otherwise: calls `migrate_recursive()` ([`mod.rs:315-442`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/mod.rs#L315-L442))

Both paths perform detection (six detectors scan for skills, commands, agents, MCP servers, hooks, and output styles), emission (writing plugin directories under `.ai/`), and registration (updating `marketplace.json`). Neither path deletes or modifies the original `.claude/` source files.

**Key code path -- original files are only read, never written or deleted:**
- Detectors call `fs.read_file()` and `fs.read_dir()` on `.claude/` contents
- Emitter calls `fs.write_file()` and `fs.create_dir_all()` only under `.ai/`
- The `Outcome` struct returns `Action` variants: `PluginCreated`, `MarketplaceRegistered`, `Renamed`, `Skipped`, `DryRunReport` -- there is no `SourceRemoved` or `Cleaned` variant

**Options struct** ([`mod.rs:213-228`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/mod.rs#L213-L228)):
```rust
pub struct Options<'a> {
    pub dir: &'a Path,
    pub source: Option<&'a str>,
    pub dry_run: bool,
    pub max_depth: Option<usize>,
    pub manifest: bool,
}
```

No `destructive` field exists on this struct.

### 2. Existing Wizard / Interactive Prompt System

The project uses the `inquire` crate (v0.9) with a well-defined two-layer architecture:

**Layer 1: Pure logic** (`wizard.rs`) -- defines prompt steps, answer types, validation, and resolution as pure functions with no I/O. Fully tested via `insta` snapshots.

**Layer 2: TTY bridge** (`wizard_tty.rs`) -- thin bridge that executes prompts against the terminal via `inquire`. Excluded from coverage.

**Existing wizard in `aipm init`:**
- [`crates/aipm/src/wizard.rs:12-238`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/aipm/src/wizard.rs#L12-L238) -- prompt definitions, answer resolution, validation, theming
- [`crates/aipm/src/wizard_tty.rs:25-93`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/aipm/src/wizard_tty.rs#L25-L93) -- TTY execution

**Interactive mode detection pattern** ([`main.rs:235`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/aipm/src/main.rs#L235)):
```rust
let interactive = !flags.yes && std::io::stdin().is_terminal();
```

This checks both the `--yes` flag and whether stdin is a terminal (piped/CI contexts are automatically non-interactive).

**Confirmation prompt pattern** ([`wizard.rs:110-115`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/aipm/src/wizard.rs#L110-L115)):
```rust
PromptKind::Confirm { default: true }
```
Executed via `inquire::Confirm::new(step.label).with_default(*default)` in [`wizard_tty.rs:64-71`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/aipm/src/wizard_tty.rs#L64-L71).

**The `aipm migrate` command currently has NO wizard, NO interactive prompts, and NO `--yes` flag.** There is no interactivity detection in `cmd_migrate()`.

### 3. CLI Flag Parsing Patterns

All CLI args use clap v4 derive macros ([`Cargo.toml:28`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/Cargo.toml#L28)):

**Boolean flag pattern:**
```rust
#[arg(long)]
dry_run: bool,
```
Automatically becomes `--dry-run` in kebab-case. Defaults to `false`, set to `true` when present.

**Subcommand-specific flags:** Defined directly as fields on `Commands` enum variants (no separate `Args` structs). Each subcommand has its own set of flags.

**Flag flow to handlers** ([`main.rs:513-514`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/aipm/src/main.rs#L513-L514)):
```rust
Some(Commands::Migrate { dry_run, source, max_depth, manifest, dir }) => {
    cmd_migrate(dry_run, source.as_deref(), max_depth, manifest, dir)
},
```

The `cmd_migrate()` handler ([`main.rs:448-485`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/aipm/src/main.rs#L448-L485)) constructs a `libaipm::migrate::Options` struct and passes it to the library.

**No mutually exclusive flags exist in the codebase.** Soft mutual exclusivity is handled at the application logic level (e.g., `--workspace`/`--marketplace` defaulting in `wizard.rs:191-201`).

### 4. Destructive / Cleanup File Operation Patterns

**Filesystem abstraction** ([`crates/libaipm/src/fs.rs:25-84`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/fs.rs#L25-L84)):

The `Fs` trait includes `remove_file()` and `remove_dir_all()` methods with safe defaults that return errors if not overridden. The `Real` implementation delegates to `std::fs`. This means any cleanup code in the migrate module can use the existing `Fs` trait for testable file deletion.

**Existing deletion patterns in the codebase:**

| Pattern | Location | Description |
|---------|----------|-------------|
| Remove-then-recreate | [`linker/hard_link.rs:32-39`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/linker/hard_link.rs#L32-L39) | `remove_dir_all` + `create_dir_all` for clean assembly state |
| Ordered two-tier unlink | [`linker/pipeline.rs:59-78`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/linker/pipeline.rs#L59-L78) | Symlink removed first, then assembled directory |
| Bulk removal via diff | [`installer/pipeline.rs:379-406`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/installer/pipeline.rs#L379-L406) | Packages removed if in old lockfile but not in new resolution |
| Atomic write with backup | [`fs.rs:167-213`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/fs.rs#L167-L213) | Backup-then-swap on Windows for safe file overwriting |

**Key observation:** No existing deletion operation in the codebase has a user-facing confirmation prompt. All destructive actions are either implicit consequences of user commands (`unlink`, `install` with changed dependencies) or internal implementation details. The `--yes` flag exists only on `init` to skip the init wizard, not to bypass safety checks.

### 5. Source Directories Tracked During Migration

The migrate pipeline already tracks which source directories were discovered:

**Single-source mode:** The source directory path is computed from `opts.source` at [`mod.rs:260`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/mod.rs#L260).

**Recursive mode:** `discover_claude_dirs()` returns `Vec<DiscoveredSource>` where each entry contains the full path to a `.claude/` directory ([`discovery.rs:33-96`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/discovery.rs#L33-L96)).

This means the information needed to know *what to delete* is already available in the pipeline -- it would need to be surfaced through the `Outcome` or handled as a post-migration step.

### 6. Action Enum (Outcome Reporting)

The `Action` enum ([`mod.rs`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/mod.rs)) currently has these variants:
- `PluginCreated { name, path, kind }`
- `MarketplaceRegistered { name }`
- `Renamed { original, resolved }`
- `Skipped { name, reason }`
- `DryRunReport { path }`

There is no variant for source cleanup/removal. A new variant would be needed (e.g., `SourceRemoved { path }` or `SourceCleaned { paths }`).

### 7. Interaction Between `--destructive` and `--dry-run`

The `--dry-run` flag gates all writes via an early return ([`mod.rs:277-282`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/mod.rs#L277-L282)):
```rust
if dry_run {
    // generate report, return early
}
```

If both `--destructive` and `--dry-run` are passed, the dry-run report could indicate what *would* be deleted without actually deleting anything.

### 8. BDD Feature File

[`tests/features/manifest/migrate.feature`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/tests/features/manifest/migrate.feature) contains scenarios for:
- Single skill migration
- `--manifest` flag
- Dry-run mode
- Recursive discovery
- Name conflict resolution
- Legacy command conversion
- Prerequisite validation

No scenario references `--destructive` or post-migration cleanup.

## Code References

- [`crates/aipm/src/main.rs:113-135`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/aipm/src/main.rs#L113-L135) -- Migrate subcommand CLI definition
- [`crates/aipm/src/main.rs:448-485`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/aipm/src/main.rs#L448-L485) -- `cmd_migrate()` handler
- [`crates/aipm/src/main.rs:513-514`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/aipm/src/main.rs#L513-L514) -- dispatch match arm
- [`crates/libaipm/src/migrate/mod.rs:213-228`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/mod.rs#L213-L228) -- `Options` struct
- [`crates/libaipm/src/migrate/mod.rs:235-247`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/mod.rs#L235-L247) -- `migrate()` entry point
- [`crates/libaipm/src/migrate/mod.rs:250-312`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/mod.rs#L250-L312) -- `migrate_single_source()`
- [`crates/libaipm/src/migrate/mod.rs:315-442`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/mod.rs#L315-L442) -- `migrate_recursive()`
- [`crates/libaipm/src/migrate/discovery.rs:33-96`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/discovery.rs#L33-L96) -- recursive discovery
- [`crates/libaipm/src/fs.rs:25-84`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/fs.rs#L25-L84) -- `Fs` trait with `remove_file()` and `remove_dir_all()`
- [`crates/aipm/src/wizard.rs:12-238`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/aipm/src/wizard.rs#L12-L238) -- wizard pure logic layer
- [`crates/aipm/src/wizard_tty.rs:25-93`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/aipm/src/wizard_tty.rs#L25-L93) -- wizard TTY bridge
- [`crates/aipm/src/main.rs:235`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/aipm/src/main.rs#L235) -- interactive mode detection pattern
- [`crates/libaipm/src/linker/pipeline.rs:54-78`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/linker/pipeline.rs#L54-L78) -- existing `unlink_package()` deletion pattern

## Architecture Documentation

### Two-Layer Wizard Pattern

Every wizard in the project follows:
1. `wizard.rs` -- pure functions defining prompts, answer types, resolution, and validation. No I/O. Tested with `insta` snapshots.
2. `wizard_tty.rs` -- thin bridge executing prompts via `inquire`. Excluded from coverage via `--ignore-filename-regex`.

The pattern for adding a new wizard step:
- Define `PromptStep` with `PromptKind::Confirm { default }` in `wizard.rs`
- Add resolution logic in a `resolve_*_answers()` function
- Execute via `inquire::Confirm::new()` in `wizard_tty.rs`
- Gate interactivity with `!flag && std::io::stdin().is_terminal()`

### Filesystem Abstraction

All file operations in `libaipm` go through the `Fs` trait. The `remove_file()` and `remove_dir_all()` methods have safe defaults (return errors) so test implementations don't accidentally delete files. Production uses `fs::Real`.

### Action-Based Outcome Reporting

The migrate pipeline returns `Outcome { actions: Vec<Action> }`. The CLI handler iterates actions to print human-readable messages. New action types require adding an enum variant and a corresponding output format in `cmd_migrate()`.

## Historical Context (from research/)

- `research/docs/2026-03-23-aipm-migrate-command.md` -- Original research documenting the copy-vs-move design decision. States: "Copy is safer and non-destructive" as the rationale for current behavior.
- `specs/2026-03-23-aipm-migrate-command.md` -- Original spec with acceptance criteria stating migration is non-destructive (line 55: "copied (non-destructive)").
- `research/docs/2026-03-22-rust-interactive-cli-prompts.md` -- Research on the `inquire` crate selection for interactive prompts.
- `specs/2026-03-22-interactive-init-wizard.md` -- Spec for the init wizard architecture (two-layer pattern).
- `research/docs/2026-03-26-install-update-link-lockfile-implementation.md` -- Documents unlink cleanup patterns.

## Related Research

- `research/docs/2026-03-23-aipm-migrate-command.md` -- Migration command design and rationale
- `research/docs/2026-03-23-recursive-claude-discovery-parallel-migrate.md` -- Recursive discovery architecture
- `research/docs/2026-03-22-rust-interactive-cli-prompts.md` -- Interactive prompt library selection
- `research/docs/2026-03-24-migrate-all-artifact-types.md` -- All artifact type support in migrate

## Open Questions

1. **Scope of deletion:** Should `--destructive` remove the entire `.claude/` directory, or only the specific files that were successfully migrated? (e.g., if only skills were migrated, should hooks config in `.claude/settings.json` be preserved?)
2. **Partial failure:** If migration succeeds for 3 of 5 artifacts but fails for 2, should `--destructive` still remove the successfully-migrated source files? Or should it be all-or-nothing?
3. **Interaction with `--dry-run`:** Should `--dry-run --destructive` show what would be deleted in the report, or should the two flags be mutually exclusive?
4. **Interaction with `--source`:** When `--source .claude` is explicitly given, only one directory is targeted. In recursive mode, multiple `.claude/` directories may be found. Should `--destructive` apply to all discovered directories?
5. **Wizard default:** Should the post-migration cleanup prompt default to "no" (safer) or "yes" (more convenient)?
6. **Non-TTY behavior:** When running without a terminal (piped/CI), and `--destructive` is not passed, the wizard cannot prompt. Should it silently skip cleanup (safe default) or require an explicit flag?
