# Fix `aipm migrate` EISDIR Crash and Add Migration Pipeline Logging

| Document Metadata      | Details                                                              |
| ---------------------- | -------------------------------------------------------------------- |
| Author(s)              | Sean Larkin                                                          |
| Status                 | Draft (WIP)                                                          |
| Team / Owner           | aipm                                                                 |
| Created / Last Updated | 2026-04-07                                                           |
| Issue                  | [#313](https://github.com/TheLarkInn/aipm/issues/313)               |
| Research               | `research/docs/2026-04-07-313-migrate-eisdir-crash.md`               |

## 1. Executive Summary

`aipm migrate` (recursive, without `--source`) crashes with "Is a directory (os error 21)" on monorepos. The dry-run and single-source paths work fine. Root cause: the emit phase performs `fs.read_to_string()` on paths that may resolve to directories (e.g., symlinks to directories), and the `Fs` trait provides no `is_file()` guard. The error message contains zero path context, and the migration pipeline has near-zero diagnostic logging (8 tracing calls across ~20 files). This spec addresses both: (A) fix the EISDIR crash by adding `is_file()` to the `Fs` trait and guarding all emitter read/write sites, and (B) add structured tracing at debug/trace levels across the full migration pipeline.

## 2. Context and Motivation

### 2.1 Current State

The migration pipeline (`crates/libaipm/src/migrate/`) converts `.claude/` and `.github/` directory structures into `.ai/` marketplace plugins. The recursive code path uses `discover_source_dirs` to find source directories, runs detectors in parallel via rayon, reconciles unclaimed files, then emits plugins and registers them.

The `Fs` trait (`crates/libaipm/src/fs.rs`) provides `exists()`, `read_to_string()`, `write_file()`, and `read_dir()` — but no `is_file()` or `is_dir()` methods. The `DirEntry.is_dir` boolean from `read_dir()` is the only mechanism to distinguish files from directories.

The `collect_files_recursive` function (`crates/libaipm/src/migrate/skill_common.rs:172`) is the single gatekeeper that prevents directory paths from entering file lists. It filters using `DirEntry.is_dir`, which does not detect symlinks to directories (Rust's `DirEntry::file_type().is_dir()` returns `false` for symlinks).

([Research reference: research/docs/2026-04-07-313-migrate-eisdir-crash.md, Sections 2-6](research/docs/2026-04-07-313-migrate-eisdir-crash.md))

### 2.2 The Problem

- **User Impact:** `aipm migrate` crashes on monorepos during recursive discovery, blocking adoption. The error message "Is a directory (os error 21)" provides no actionable information.
- **Debugging Impact:** The migration pipeline has only 8 tracing calls across ~20 source files. The core orchestration functions (`migrate`, `migrate_recursive`, `emit_and_register`), all detectors (except one), the reconciler, and the registrar have zero logging. Even with `AIPM_LOG=trace`, the log file reveals nothing about the crash.
- **Technical Debt:** 19 of 20 `fs.read_to_string`/`fs.write_file` call sites in the emitter use bare `?` propagation with no `.map_err()` — raw `std::io::Error` bubbles up through `#[error(transparent)]` with zero path context.

## 3. Goals and Non-Goals

### 3.1 Functional Goals

- [ ] `aipm migrate` (recursive) no longer crashes with EISDIR when source directories contain symlinks to directories, named pipes, or other non-regular-file entries
- [ ] When a non-file entry is encountered during emission, it is skipped with a `tracing::warn!` message that includes the offending path
- [ ] All IO errors in the migration pipeline include the path and operation in the error message
- [ ] The migration pipeline has structured tracing at `debug` and `trace` levels across: discovery, detection, reconciliation, emission, and registration
- [ ] The `Fs` trait gains an `is_file()` method with a default implementation
- [ ] All existing tests continue to pass; new tests cover the `is_file()` guard and skip-on-EISDIR behavior
- [ ] Coverage remains at or above the 89% branch threshold

### 3.2 Non-Goals (Out of Scope)

- [ ] We will NOT change the migration pipeline's functional behavior (what gets migrated, how plugins are structured)
- [ ] We will NOT add `#[instrument]` attributes in this PR (may be a follow-up)
- [ ] We will NOT add logging to detectors in this PR (scope limited to the pipeline spine and emitter)
- [ ] We will NOT change the `Error` enum variants — we wrap IO errors with context strings, not new enum arms

## 4. Proposed Solution (High-Level Design)

### 4.1 Architecture

No new crates, modules, or architectural changes. This is a defensive-fix + observability enhancement within the existing `libaipm` crate.

```
[Fs trait]                      Add is_file() method
     |
[collect_files_recursive]       Use is_file() as secondary guard
     |
[emitter: all fs.* call sites]  Guard with is_file(), skip+warn on non-file
     |
[pipeline: mod.rs, discovery]   Add tracing at debug/trace levels
     |
[Error::Io]                     Wrap with path context via .map_err()
```

### 4.2 Key Components

| Component | Change | Files |
|-----------|--------|-------|
| `Fs` trait | Add `is_file(&self, path: &Path) -> bool` with default impl | `crates/libaipm/src/fs.rs` |
| `collect_files_recursive` | Add `is_file()` guard after `!is_dir` check | `crates/libaipm/src/migrate/skill_common.rs` |
| Emitter call sites | Guard reads with `is_file()`, skip+warn on non-file; wrap `?` with `.map_err()` for path context | `crates/libaipm/src/migrate/emitter.rs` |
| Pipeline orchestration | Add `tracing::debug!` / `tracing::trace!` at pipeline boundaries | `crates/libaipm/src/migrate/mod.rs` |
| Discovery | Add `tracing::trace!` to `discover_source_dirs` (match sibling `discover_features`) | `crates/libaipm/src/discovery.rs` |
| Reconciler | Add `tracing::debug!` summary after reconciliation | `crates/libaipm/src/migrate/reconciler.rs` |
| Registrar | Add `tracing::debug!` for registration | `crates/libaipm/src/migrate/registrar.rs` |
| Cleanup | Minimal (already has 1 debug call) | `crates/libaipm/src/migrate/cleanup.rs` |

## 5. Detailed Design

### 5.1 Add `is_file()` to the `Fs` Trait

**File:** `crates/libaipm/src/fs.rs`

Add to the trait definition (after `exists`):

```rust
/// Check if a path is a regular file (not a directory, symlink-to-directory, or special file).
fn is_file(&self, path: &Path) -> bool {
    // Default: conservative — defer to exists()
    self.exists(path)
}
```

Implement in `Real`:

```rust
fn is_file(&self, path: &Path) -> bool {
    path.is_file()
}
```

`std::path::Path::is_file()` follows symlinks and returns `true` only for regular files. Symlinks to directories return `false`. This is exactly the guard we need.

**Mock implementations:** Add `is_file` to each mock `Fs` in the test modules. For existing mocks that track an `exists: HashSet<PathBuf>`, `is_file` can reuse `self.exists.contains(path)` (existing mock behavior — tests don't use symlinks). Mocks that need to simulate the "is a directory" case can add a `dirs: HashSet<PathBuf>` and return `self.exists.contains(path) && !self.dirs.contains(path)`.

### 5.2 Guard `collect_files_recursive`

**File:** `crates/libaipm/src/migrate/skill_common.rs:172-191`

Add an `is_file()` check as a secondary guard after `!is_dir`:

```rust
pub fn collect_files_recursive(
    dir: &Path,
    base: &Path,
    fs: &dyn Fs,
) -> Result<Vec<PathBuf>, Error> {
    let mut files = Vec::new();
    let entries = fs.read_dir(dir)?;

    for entry in entries {
        let full_path = dir.join(&entry.name);
        if entry.is_dir {
            let sub_files = collect_files_recursive(&full_path, base, fs)?;
            files.extend(sub_files);
        } else if fs.is_file(&full_path) {
            if let Ok(relative) = full_path.strip_prefix(base) {
                files.push(relative.to_path_buf());
            }
        } else {
            tracing::warn!(
                path = %full_path.display(),
                "skipping non-regular file during migration discovery"
            );
        }
    }

    Ok(files)
}
```

This catches symlinks-to-directories, named pipes, device files, and any other non-regular-file entry at the collection point, before they reach any emitter.

### 5.3 Guard and Annotate Emitter Call Sites

**File:** `crates/libaipm/src/migrate/emitter.rs`

**Strategy:** For each of the 19 bare-`?` call sites, apply one of two patterns:

#### Pattern A: Skip + warn (for file-iteration sites like `emit_other_files`, `emit_skill_files`)

Model after the existing `emit_extension_files` pattern at line 619:

```rust
// Before (emit_other_files:747-750):
if fs.exists(&file.path) {
    let content = fs.read_to_string(&file.path)?;
    fs.write_file(&dest, content.as_bytes())?;
}

// After:
if fs.exists(&file.path) {
    if !fs.is_file(&file.path) {
        tracing::warn!(
            path = %file.path.display(),
            "skipping non-regular file during other-file migration"
        );
        continue;
    }
    let content = fs.read_to_string(&file.path).map_err(|e| {
        Error::Io(std::io::Error::new(
            e.kind(),
            format!("reading {}: {e}", file.path.display()),
        ))
    })?;
    fs.write_file(&dest, content.as_bytes()).map_err(|e| {
        Error::Io(std::io::Error::new(
            e.kind(),
            format!("writing {}: {e}", dest.display()),
        ))
    })?;
}
```

Apply this skip+warn pattern to these call sites (all iterate over file lists):
- `emit_skill_files` (line 146, 151) — loop over `artifact.files`
- `emit_other_files` (line 749, 750) — loop over `other_files`
- `copy_referenced_scripts` (line 685, 686) — loop over `referenced_scripts`

#### Pattern B: Annotate with path context (for single-file reads)

For call sites that read a single artifact's `source_path` (not iterating), just add `.map_err()` for context. These paths come from detectors that already guard with `is_dir`, so the risk is lower:

```rust
// Before (emit_agent_files:515):
let content = fs.read_to_string(&artifact.source_path)?;

// After:
let content = fs.read_to_string(&artifact.source_path).map_err(|e| {
    Error::Io(std::io::Error::new(
        e.kind(),
        format!("reading agent {}: {e}", artifact.source_path.display()),
    ))
})?;
```

Apply this annotate pattern to:
- `emit_command_as_skill` (line 161) — `"reading command {path}"`
- `emit_agent_files` (line 515) — `"reading agent {path}"`
- `emit_output_style` (line 557) — `"reading output style {path}"`
- `emit_mcp_config` (line 529) — `"reading mcp config {path}"`
- `emit_hooks_config` (line 543) — `"reading hooks config {path}"`
- `emit_lsp_config` (line 569) — `"reading lsp config {path}"`

Also annotate `write_file` calls in these functions with `"writing {dest_path}"`.

### 5.4 Add Pipeline Tracing

All tracing uses fully-qualified `tracing::level!()` calls (no imports). Follow existing field conventions: `path = %path.display()`, `error = %e`, message last, lowercase, no trailing period.

#### `discover_source_dirs` — `crates/libaipm/src/discovery.rs:101`

Match the pattern from sibling `discover_features` (lines 294-338):

```rust
// After building walker, before iteration:
tracing::trace!("starting source directory discovery");

// Inside the loop, after matching a pattern:
tracing::trace!(dir = %source_dir.display(), source_type = source_type_str, "discovered source directory");

// After the loop:
tracing::trace!(total = discovered.len(), "source directory discovery complete");
```

#### `migrate` — `crates/libaipm/src/migrate/mod.rs:322`

```rust
// At entry:
tracing::debug!(
    source = ?opts.source,
    dry_run = opts.dry_run,
    destructive = opts.destructive,
    "starting migration"
);
```

#### `migrate_recursive` — `crates/libaipm/src/migrate/mod.rs:443`

```rust
// After discover_source_dirs returns:
tracing::debug!(discovered = discovered.len(), "discovered source directories for recursive migration");

// After detection results are collected:
tracing::debug!(plans = plugin_plans.len(), "detection complete");

// Before dry-run early return:
tracing::debug!("dry-run mode — generating report");

// Before emit_and_register:
tracing::debug!("emitting plugins");
```

#### `emit_and_register` — `crates/libaipm/src/migrate/mod.rs:531`

```rust
// At entry:
tracing::debug!(plans = resolved.len(), "starting plugin emission");

// For each plan emission (inside the par_iter, at trace level since it's per-item):
tracing::trace!(plugin = final_name, artifacts = plan.artifacts.len(), other_files = plan.other_files.len(), "emitting plugin");

// After all emissions:
tracing::debug!(emitted = all_actions.len(), registered = registered_entries.len(), "emission and registration complete");
```

#### `reconcile` — `crates/libaipm/src/migrate/reconciler.rs:23`

```rust
// After collecting all files and building claimed set:
tracing::debug!(
    total_files = all_files.len(),
    claimed = claimed.len(),
    unclaimed = unclaimed.len(),
    source_dir = %source_dir.display(),
    "reconciliation complete"
);
```

#### `register_plugins` — `crates/libaipm/src/migrate/registrar.rs:10`

```rust
// At entry:
tracing::debug!(count = entries.len(), "registering plugins in marketplace.json");
```

#### `emit_other_files` — `crates/libaipm/src/migrate/emitter.rs:699`

```rust
// At entry:
tracing::trace!(count = other_files.len(), plugin_dir = %plugin_dir.display(), "emitting other files");

// Per file (inside the loop):
tracing::trace!(path = %file.path.display(), dest = %dest.display(), "copying other file");
```

### 5.5 Tests

#### New unit tests for `Fs::is_file()`

**File:** `crates/libaipm/src/fs.rs` (in the `#[cfg(test)]` block)

- `real_is_file_returns_true_for_regular_file` — create a file, assert `Real.is_file()` returns `true`
- `real_is_file_returns_false_for_directory` — create a dir, assert `Real.is_file()` returns `false`
- `real_is_file_returns_false_for_symlink_to_directory` — create a dir, symlink to it, assert `Real.is_file()` returns `false`
- `real_is_file_returns_true_for_symlink_to_file` — create a file, symlink to it, assert `Real.is_file()` returns `true`
- `real_is_file_returns_false_for_nonexistent` — assert `Real.is_file()` returns `false` for a path that doesn't exist

#### New unit test for `collect_files_recursive` symlink handling

**File:** `crates/libaipm/src/migrate/skill_common.rs` (in the `#[cfg(test)]` block)

Add a mock that returns a `DirEntry { name: "link", is_dir: false }` where the corresponding path is NOT a file (mock `is_file` returns `false`). Assert the entry is excluded from the collected files list.

#### New unit test for `emit_other_files` skip behavior

**File:** `crates/libaipm/src/migrate/emitter.rs` (in the `#[cfg(test)]` block)

Add a test where an `OtherFile` has a path that exists but `is_file()` returns `false`. Assert the function completes successfully (no error), the file is skipped, and an `OtherFileMigrated` action is still emitted (or skipped — match the decided behavior).

#### Existing test updates

All existing `MockFs` implementations across the test modules gain a trivial `is_file` implementation. Since existing mocks don't track directory state separately, `is_file` can return `self.exists.contains(path)` — this preserves current test behavior.

## 6. Alternatives Considered

| Option | Pros | Cons | Reason for Rejection |
|--------|------|------|---------------------|
| Add `metadata()` to `Fs` returning file type, size, etc. | More future-proof, richer API | Much larger API surface change, every mock needs a new method returning a struct | Over-engineered for this fix; `is_file()` is sufficient |
| Only wrap errors (no `is_file` guard) | Smaller change, identifies the crash site | Still crashes — just with a better message; user must still re-run after fixing the offending file | We want skip+warn, not fail-with-context |
| Add `--strict` flag for configurable behavior | Flexibility | Added complexity, more code paths to test, not requested | YAGNI — skip+warn is the right default; can add later if needed |
| Use `DirEntry` with resolved file type (follow symlinks) | Fixes the root cause at the source | Changes `read_dir` semantics for all callers, may have unintended effects in lint/install | Too broad a change; `is_file` guard is surgical |

## 7. Cross-Cutting Concerns

### 7.1 Observability Strategy

- **Log levels:** `trace` for per-item iteration (each file, each directory), `debug` for pipeline milestones (discovery complete, emission complete, reconciliation summary), `warn` for skipped entries
- **Field conventions:** Match existing codebase patterns — `path = %path.display()`, `error = %e`, `count = n`, message last, lowercase
- **File log:** Always captures `debug` level (existing behavior via `logging.rs`), so pipeline milestones appear in `<temp_dir>/aipm-*.log` without any user action
- **Stderr log:** Controlled by `AIPM_LOG` env var or `-v`/`-vv` flags; users debugging issues set `AIPM_LOG=trace`

### 7.2 Backwards Compatibility

- `Fs::is_file()` has a default implementation (`self.exists(path)`), so existing external implementations (if any) are not broken
- No new CLI flags, no changed output format, no changed exit codes (except: previously-crashing runs now succeed with warnings)
- Mock implementations in tests get a trivial `is_file` that reuses `exists` — no test behavior change

## 8. Migration, Rollout, and Testing

### 8.1 Deployment Strategy

Ship in a single PR. No feature flags needed — the fix is purely defensive (skip+warn instead of crash).

### 8.2 Test Plan

- **Unit Tests:** New tests for `is_file()` on `Real`, `collect_files_recursive` with symlink-like entries, `emit_other_files` skip behavior
- **Existing Tests:** All pass with `is_file` added to mocks; run `cargo test --workspace`
- **Lint:** `cargo clippy --workspace -- -D warnings` (no new `#[allow]`)
- **Format:** `cargo fmt --check`
- **Coverage:** `cargo +nightly llvm-cov report --doctests --branch --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs)'` — must show >= 89% branch coverage
- **Manual verification:** Run `aipm migrate --dry-run` and `aipm migrate` on a test monorepo with a symlinked directory inside `.claude/` — confirm skip+warn behavior instead of crash

## 9. Open Questions / Unresolved Issues

- [ ] Should skipped non-regular files produce a visible CLI warning (via `writeln!(stderr, ...)`) in addition to the `tracing::warn!`? The tracing warn goes to the log file and stderr-if-verbose, but the user might not see it at default verbosity. **Recommendation:** Add a `Action::Skipped` action for non-regular files so it surfaces in the CLI output naturally.
- [ ] Should the `OtherFileMigrated` action still be emitted for skipped files, or should we omit it? **Recommendation:** Omit it — only emit for files that were actually copied. The `Action::Skipped` covers the skip case.
