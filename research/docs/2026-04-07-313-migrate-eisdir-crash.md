---
date: 2026-04-07
researcher: Claude
git_commit: d8783d6b889c60393524d934006c6172613bf217
branch: main
repository: aipm
topic: "Issue #313: aipm migrate crashes with 'Is a directory (os error 21)' on recursive run"
tags: [research, codebase, migrate, eisdir, discovery, logging]
status: complete
last_updated: 2026-04-07
last_updated_by: Claude
---

# Research: Issue #313 — `aipm migrate` EISDIR Crash

## Research Question

What code path in the recursive migration (no `--source`) differs from the single-source and dry-run paths, and where could it attempt a file operation on a directory (triggering EISDIR/os error 21)? Additionally, is there adequate logging to debug the discovery and emission algorithms?

## Summary

The EISDIR error originates in the **emit phase** of the recursive migration pipeline, which is the only phase that runs in the non-dry-run recursive path but not in the dry-run or single-source paths. The error propagates through `migrate::Error::Io` with `#[error(transparent)]`, which **strips all context** — no file path, no function name, no operation type — making diagnosis impossible from the error message alone.

The logging coverage across the migration pipeline is **critically sparse**: only 8 tracing calls exist across ~20 source files. The core pipeline orchestration functions (`migrate`, `migrate_recursive`, `emit_and_register`), all 11 detectors (except copilot_extension_detector), the reconciler, and the registrar have **zero logging**. The `discover_source_dirs` function used by recursive mode also has zero logging.

## Detailed Findings

### 1. How the three code paths diverge

| Path | Discovery | Detection | Reconcile | Emit | Register |
|------|-----------|-----------|-----------|------|----------|
| `--dry-run` (recursive) | `discover_source_dirs` | Parallel detectors | `reconciler::reconcile` | **Skipped** (returns at [mod.rs:515](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/migrate/mod.rs#L515)) | Skipped |
| `--source .github` | Manual path join | Sequential detectors | `reconciler::reconcile` | `emit_plugin` per artifact | `register_plugins` |
| Recursive (no flags) | `discover_source_dirs` | Parallel detectors (rayon) | `reconciler::reconcile` | **`emit_and_register`** (parallel rayon) | `register_plugins` |

The dry-run never reaches the emit phase. The single-source path processes only the specified directory. The recursive path discovers ALL `.claude/` and `.github/` directories and emits them in parallel.

### 2. Where EISDIR can be triggered

All filesystem operations ultimately go through the `Fs` trait ([fs.rs:25-84](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/fs.rs#L25-L84)). The `Real` implementation delegates directly to `std::fs`. On Linux, these operations produce EISDIR (os error 21) when given a directory path:

- `std::fs::read_to_string(dir_path)` → EISDIR
- `std::fs::File::create(dir_path)` → EISDIR (used by `Real::write_file`)

#### Risk-ranked locations in the emit phase

**HIGH — `emit_other_files`** ([emitter.rs:747-750](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/migrate/emitter.rs#L747-L750)):
```rust
if fs.exists(&file.path) {
    let content = fs.read_to_string(&file.path)?;
    fs.write_file(&dest, content.as_bytes())?;
}
```
- `file.path` comes from `collect_files_recursive` → `collect_all_files` in the reconciler
- The `collect_files_recursive` function ([skill_common.rs:172-191](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/migrate/skill_common.rs#L172-L191)) filters directories via `DirEntry.is_dir`, but **symlinks to directories have `is_dir = false`** in Rust's `DirEntry::file_type()`, so they pass through as "files"
- `fs.exists()` follows symlinks and returns `true` for symlinked directories
- `fs.read_to_string()` follows the symlink, finds a directory → EISDIR

**HIGH — `emit_skill_files`** ([emitter.rs:146](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/migrate/emitter.rs#L146)):
```rust
let content = fs.read_to_string(&source)?;
```
- `source` from `artifact.files` populated by `collect_files_recursive` — same symlink vulnerability

**MEDIUM — `copy_referenced_scripts`** ([emitter.rs:678-686](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/migrate/emitter.rs#L678-L686)):
```rust
if fs.exists(&source) {
    let content = fs.read_to_string(&source)?;
    fs.write_file(&dest, content.as_bytes())?;
}
```
- `source` from `artifact.referenced_scripts` (parsed from markdown content)
- Has `fs.exists()` check but no `is_file()` check

**MEDIUM — `emit_command_as_skill`** ([emitter.rs:161](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/migrate/emitter.rs#L161)), **`emit_agent_files`** ([emitter.rs:515](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/migrate/emitter.rs#L515)), **`emit_output_style`** ([emitter.rs:557](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/migrate/emitter.rs#L557)):
- All call `fs.read_to_string(&artifact.source_path)` where `source_path` was set by detectors using `is_dir` guards that wouldn't catch symlinks

**LOW — `emit_mcp_config`, `emit_hooks_config`, `emit_lsp_config`** ([emitter.rs:526-574](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/migrate/emitter.rs#L526-L574)):
- Fall back to `fs.read_to_string(&artifact.source_path)` only when `raw_content` is `None`
- Source paths are hardcoded filenames (e.g., `settings.json`, `.mcp.json`) — low risk

### 3. The `Fs` trait lacks `is_file()`

The `Fs` trait ([fs.rs:25-84](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/fs.rs#L25-L84)) provides `exists()` but no `is_file()` or `is_dir()` method. The only way to distinguish files from directories is through the `DirEntry.is_dir` field returned by `read_dir()`. This means:
- No code can perform a secondary filesystem check before `read_to_string` or `write_file`
- Symlinks to directories are invisible — they pass `!is_dir` but fail on read

### 4. Error context is completely stripped

The error chain:
1. `std::io::Error { kind: IsADirectory, message: "Is a directory (os error 21)" }` — raw OS error
2. `migrate::Error::Io(io_error)` — via `#[from] std::io::Error` ([mod.rs:293-294](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/migrate/mod.rs#L293-L294))
3. `#[error(transparent)]` — passes through the raw message with zero added context
4. CLI prints `error: Is a directory (os error 21)` ([main.rs:1017](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/aipm/src/main.rs#L1017))

No information about which file, function, source directory, or plugin plan caused the error.

### 5. Logging coverage audit

**Framework**: `tracing = "0.1"` with dual-layer subscriber — stderr (controlled by `AIPM_LOG` env var or CLI verbosity) + file layer (always DEBUG, daily rotation, `<temp_dir>/aipm-*.log`). Setup in [logging.rs:67-112](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/logging.rs#L67-L112).

| Level | Count | Where |
|-------|-------|-------|
| `trace!` | 4 calls | `discovery.rs` only (`discover_features` walk) |
| `debug!` | 3 calls | `cleanup.rs` (1), `copilot_extension_detector.rs` (2) |
| `info!` | 0 | None in migration/discovery |
| `warn!` | 1 call | `emitter.rs` (extension file read failure) |
| `error!` | 0 | None in migration/discovery |
| **Total** | **8 calls** | across ~20 source files |

**Zero `#[instrument]` attributes** in the entire codebase.

**Functions with ZERO logging** (critical path highlighted):

| Function | File | Why it matters |
|----------|------|----------------|
| **`migrate()`** | [mod.rs:322](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/migrate/mod.rs#L322) | Top-level entry — no mode selection log |
| **`migrate_recursive()`** | [mod.rs:443](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/migrate/mod.rs#L443) | No log of discovered dirs or plan count |
| **`emit_and_register()`** | [mod.rs:531](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/migrate/mod.rs#L531) | No log of which plugins are being emitted |
| **`discover_source_dirs()`** | [discovery.rs:101](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/discovery.rs#L101) | Zero logging (contrast: sibling `discover_features` has 4 trace calls) |
| **`emit_other_files()`** | [emitter.rs:699](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/migrate/emitter.rs#L699) | No log of which files are being read/written |
| **`reconcile()`** | [reconciler.rs:23](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/migrate/reconciler.rs#L23) | No log of file counts or unclaimed files |
| **`collect_files_recursive()`** | [skill_common.rs:172](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/migrate/skill_common.rs#L172) | No log of entries found, dirs recursed, files collected |
| **`register_plugins()`** | [registrar.rs:10](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/migrate/registrar.rs#L10) | No log of which plugins are being registered |
| All 10 detectors (except copilot_extension_detector) | `*_detector.rs` | No log of what they scan, find, or skip |

### 6. `collect_files_recursive` — the single gatekeeper

[skill_common.rs:172-191](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/migrate/skill_common.rs#L172-L191) is the single function responsible for preventing directory paths from entering file lists:

```rust
pub fn collect_files_recursive(dir: &Path, base: &Path, fs: &dyn Fs) -> Result<Vec<PathBuf>, Error> {
    let mut files = Vec::new();
    let entries = fs.read_dir(dir)?;
    for entry in entries {
        let full_path = dir.join(&entry.name);
        if entry.is_dir {
            let sub_files = collect_files_recursive(&full_path, base, fs)?;
            files.extend(sub_files);
        } else if let Ok(relative) = full_path.strip_prefix(base) {
            files.push(relative.to_path_buf());
        }
    }
    Ok(files)
}
```

The `entry.is_dir` check depends on `Real::read_dir()` ([fs.rs:109-120](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/fs.rs#L109-L120)), which uses `entry.file_type()?.is_dir()`. On Linux:
- Regular directories: `is_dir() = true` → recursed into (correct)
- Regular files: `is_dir() = false` → added to list (correct)
- **Symlinks to directories**: `file_type().is_dir() = false` → **added to list as a file** (BUG — will cause EISDIR when read)
- **Symlinks to files**: `file_type().is_dir() = false` → added to list as a file (correct — read follows symlink to the file)

### 7. Difference between dry-run and emit paths

The dry-run path ([mod.rs:515-524](https://github.com/TheLarkInn/aipm/blob/d8783d6b889c60393524d934006c6172613bf217/crates/libaipm/src/migrate/mod.rs#L515-L524)) only accesses `OtherFile` metadata (name, path, associations) for report generation. It **never calls `fs.read_to_string()`** on the other file paths. This is why dry-run succeeds even when the file list contains a directory path.

## Code References

- `crates/libaipm/src/migrate/mod.rs:322` — `migrate()` entry point
- `crates/libaipm/src/migrate/mod.rs:443` — `migrate_recursive()` — recursive pipeline
- `crates/libaipm/src/migrate/mod.rs:531` — `emit_and_register()` — parallel emission
- `crates/libaipm/src/migrate/mod.rs:293-294` — `Error::Io` with `#[error(transparent)]`
- `crates/libaipm/src/discovery.rs:101` — `discover_source_dirs()` — zero logging
- `crates/libaipm/src/migrate/emitter.rs:699-761` — `emit_other_files()` — primary EISDIR risk
- `crates/libaipm/src/migrate/emitter.rs:130-154` — `emit_skill_files()` — secondary risk
- `crates/libaipm/src/migrate/emitter.rs:641-690` — `copy_referenced_scripts()` — tertiary risk
- `crates/libaipm/src/migrate/skill_common.rs:172-191` — `collect_files_recursive()` — symlink gap
- `crates/libaipm/src/fs.rs:25-84` — `Fs` trait — no `is_file()` method
- `crates/libaipm/src/fs.rs:109-120` — `Real::read_dir()` — `is_dir` from `file_type()`
- `crates/libaipm/src/logging.rs:67-112` — Logging framework setup
- `crates/aipm/src/main.rs:1017` — Error display (no context)

## Architecture Documentation

### Migration pipeline flow (recursive)
```
cmd_migrate (main.rs:834)
  → migrate (mod.rs:322)
    → migrate_recursive (mod.rs:443)
      → discover_source_dirs (discovery.rs:101)           [NO LOGGING]
      → par_iter over discovered dirs (rayon)
        → detectors_for_source → detect()                 [NO LOGGING in 10/11 detectors]
        → reconciler::reconcile                            [NO LOGGING]
          → collect_all_files → collect_files_recursive    [NO LOGGING, SYMLINK GAP]
      → if dry_run: generate_report → return               [WHY DRY-RUN WORKS]
      → emit_and_register (mod.rs:531)                     [NO LOGGING]
        → par_iter over plans (rayon)
          → emit_plugin_with_name / emit_package_plugin    [NO LOGGING]
          → emit_other_files                               [NO LOGGING, EISDIR HERE]
        → register_plugins                                 [NO LOGGING]
```

### Logging framework
- **Crate**: `tracing = "0.1"` — workspace dependency
- **Subscriber**: dual-layer — stderr (AIPM_LOG / verbosity) + file (always DEBUG, daily rotation, `<temp_dir>/aipm-*.log`)
- **Zero `#[instrument]`** attributes in the codebase

## Historical Context (from research/)

- `research/docs/2026-03-23-recursive-claude-discovery-parallel-migrate.md` — Original design for recursive discovery
- `research/docs/2026-04-01-migrate-file-discovery-classification.md` — File discovery and classification research
- `research/docs/2026-04-01-migrate-file-movement-paths.md` — File movement paths during migration
- `research/docs/2026-04-01-migrate-dry-run-report.md` — Dry-run report design

## Related Research

- `specs/2026-03-23-recursive-migrate-discovery.md` — Design spec for recursive discovery
- `specs/2026-04-01-migrate-other-files-and-dependency-tracking.md` — Other files handling spec

## Open Questions

1. **Does the user's monorepo contain symlinks inside `.claude/` or `.github/` directories?** A symlink-to-directory inside a source dir would be the simplest explanation for EISDIR.
2. **Which specific `fs.*` operation triggers the error?** Without path-annotated errors or logging, this cannot be determined from the error message alone.
3. **Does the error occur during `read_to_string` or `write_file`?** Both can trigger EISDIR for different reasons.
4. **Are there any unusual directory entries (e.g., named pipes, device files, mount points) in the source directories?** These would also cause unexpected behavior.
