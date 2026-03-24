---
date: 2026-03-23 16:02:50 PDT
researcher: Claude
git_commit: 1ffc197c68f48d6eac37e25ae65b6b9afae21f6d
branch: main
repository: aipm
topic: "Recursive .claude/ folder discovery and parallel detect+emit for aipm migrate"
tags: [research, codebase, migrate, parallelism, monorepo, recursive-scan]
status: complete
last_updated: 2026-03-23
last_updated_by: Claude
---

# Research: Recursive `.claude/` Discovery and Parallel Detect+Emit

## Research Question

How does the current migrate pipeline locate `.claude/` source directories, and what would need to change to support recursive discovery of all `.claude/` folders across the entire repo tree (including monorepo subpackages)? What existing patterns exist in the codebase for parallelism or multi-directory scanning that could inform a multithreaded detect+emit approach?

## Summary

The current `aipm migrate` pipeline scans exactly one `.claude/` directory at the project root (`opts.dir.join(opts.source)`). It does not recurse into subdirectories. The entire codebase is synchronous and single-threaded — no concurrency crates (`rayon`, `tokio`, etc.) are used, and the `Fs` trait has no `Send`/`Sync` bounds.

To support recursive `.claude/` discovery with parallel detection, three areas require changes:

1. **Discovery layer** — A new recursive scanner that walks the repo tree to find all `.claude/` directories. The `ignore` crate (from ripgrep) provides gitignore-aware parallel directory walking out of the box.
2. **Fs trait bounds** — The `Fs` trait needs `Send + Sync` supertraits (or a parallel-safe alternative) to be shared across threads. The `Real` implementation already satisfies these bounds since it's a unit struct; test mocks would need to switch from `RefCell` to `Mutex`.
3. **Emit sequencing** — The emitter uses a `&mut u32` rename counter and a progressively-growing `known_names` set. These create ordering dependencies that prevent naive parallelization of emit. Detection (read-only) can be parallelized freely; emission (writes) requires either sequencing or synchronized shared state.

## Detailed Findings

### 1. Current Source Directory Resolution

**File:** [`crates/libaipm/src/migrate/mod.rs:158-174`](https://github.com/TheLarkInn/aipm/blob/1ffc197c68f48d6eac37e25ae65b6b9afae21f6d/crates/libaipm/src/migrate/mod.rs#L158-L174)

The orchestrator `migrate()` computes a single source directory:

```rust
let source_dir = opts.dir.join(opts.source);  // e.g., /project/.claude
```

It then validates that this directory exists, selects detectors based on `opts.source` (hardcoded match on `".claude"`), and runs each detector once against that single directory.

The CLI passes `--source .claude` (default) and a single `dir` argument:

```rust
// crates/aipm/src/main.rs:46-58
Migrate {
    #[arg(long, default_value = ".claude")]
    source: String,
    #[arg(default_value = ".")]
    dir: PathBuf,
}
```

There is no `--recursive` flag, no multi-directory support, and no glob expansion.

### 2. Detector Trait: Single Source Dir Per Call

**File:** [`crates/libaipm/src/migrate/detector.rs:13-19`](https://github.com/TheLarkInn/aipm/blob/1ffc197c68f48d6eac37e25ae65b6b9afae21f6d/crates/libaipm/src/migrate/detector.rs#L13-L19)

```rust
pub trait Detector {
    fn name(&self) -> &'static str;
    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error>;
}
```

The `detect()` method accepts a single `source_dir`. It is **not** hardcoded to `.claude` — the detectors join relative child paths (`"skills"`, `"commands"`) onto whatever `source_dir` they receive. This means the same detectors can scan any `.claude/` directory without modification; only the caller needs to supply different paths.

### 3. SkillDetector and CommandDetector Internals

**SkillDetector** ([`crates/libaipm/src/migrate/skill_detector.rs:18-57`](https://github.com/TheLarkInn/aipm/blob/1ffc197c68f48d6eac37e25ae65b6b9afae21f6d/crates/libaipm/src/migrate/skill_detector.rs#L18-L57)):
- Scans `source_dir.join("skills")` with `fs.read_dir()`
- For each subdirectory containing `SKILL.md`, parses frontmatter and collects files recursively
- Uses `collect_files_recursive()` (lines 157-173) — the only recursive dir walker in the codebase
- Read-only against the filesystem (only calls `exists`, `read_dir`, `read_to_string`)

**CommandDetector** ([`crates/libaipm/src/migrate/command_detector.rs:19-63`](https://github.com/TheLarkInn/aipm/blob/1ffc197c68f48d6eac37e25ae65b6b9afae21f6d/crates/libaipm/src/migrate/command_detector.rs#L19-L63)):
- Scans `source_dir.join("commands")` with `fs.read_dir()`
- Processes each `.md` file as a command artifact
- Also read-only

Both detectors are stateless unit structs. Multiple instances could safely run concurrently on different `source_dir` paths, provided the `Fs` implementation is thread-safe.

### 4. Fs Trait: No Send/Sync Bounds

**File:** [`crates/libaipm/src/fs.rs:19-33`](https://github.com/TheLarkInn/aipm/blob/1ffc197c68f48d6eac37e25ae65b6b9afae21f6d/crates/libaipm/src/fs.rs#L19-L33)

```rust
pub trait Fs {
    fn exists(&self, path: &Path) -> bool;
    fn create_dir_all(&self, path: &Path) -> std::io::Result<()>;
    fn write_file(&self, path: &Path, content: &[u8]) -> std::io::Result<()>;
    fn read_to_string(&self, path: &Path) -> std::io::Result<String>;
    fn read_dir(&self, path: &Path) -> std::io::Result<Vec<DirEntry>>;
}
```

The trait has **no supertraits**. `dyn Fs` is neither `Send` nor `Sync`. The `Real` implementation is a unit struct (auto-derives `Send + Sync`), but the trait object `&dyn Fs` passed everywhere cannot be shared across threads without adding bounds.

Test mocks use `RefCell` (which is `!Sync`), confirming the codebase does not expect thread-safe `Fs` implementations.

### 5. Emitter: Ordering Dependencies Prevent Naive Parallelism

**File:** [`crates/libaipm/src/migrate/emitter.rs:28-34`](https://github.com/TheLarkInn/aipm/blob/1ffc197c68f48d6eac37e25ae65b6b9afae21f6d/crates/libaipm/src/migrate/emitter.rs#L28-L34)

```rust
pub fn emit_plugin<S: BuildHasher>(
    artifact: &Artifact,
    ai_dir: &Path,
    existing_names: &HashSet<String, S>,
    rename_counter: &mut u32,
    fs: &dyn Fs,
) -> Result<(String, Vec<Action>), Error>
```

Key shared mutable state:
- `rename_counter: &mut u32` — incremented on each conflict, produces sequential IDs (`-renamed-1`, `-renamed-2`)
- `known_names` (in caller, `mod.rs:204`) — `HashSet` grows after each emit, fed back as `existing_names` for the next call

The orchestrator loop at [`mod.rs:200-206`](https://github.com/TheLarkInn/aipm/blob/1ffc197c68f48d6eac37e25ae65b6b9afae21f6d/crates/libaipm/src/migrate/mod.rs#L200-L206) has each iteration depending on the previous one's output. This prevents parallel emit without restructuring.

### 6. Registrar: Read-Modify-Write on marketplace.json

**File:** [`crates/libaipm/src/migrate/registrar.rs:10`](https://github.com/TheLarkInn/aipm/blob/1ffc197c68f48d6eac37e25ae65b6b9afae21f6d/crates/libaipm/src/migrate/registrar.rs#L10)

`register_plugins()` reads `marketplace.json`, parses it, appends new entries, and writes it back. This is a non-atomic read-modify-write — called once at the end with all names. Not a parallelism concern since it's called once after all emission completes.

### 7. Existing Parallelism: None

The codebase is entirely synchronous and single-threaded:
- No `rayon`, `tokio`, `crossbeam`, or `std::thread` usage in application code
- No `Arc`, `Mutex`, `RwLock` usage
- No `async fn` or `.await` in production code
- `tokio` appears only as a transitive dependency of `reqwest`

### 8. Existing Monorepo/Workspace Concepts

**File:** [`crates/libaipm/src/manifest/types.rs:67-78`](https://github.com/TheLarkInn/aipm/blob/1ffc197c68f48d6eac37e25ae65b6b9afae21f6d/crates/libaipm/src/manifest/types.rs#L67-L78)

A `Workspace` struct exists with `members: Vec<String>` (glob patterns like `".ai/*"`) and `plugins_dir`. The `aipm init --workspace` command generates this. However, glob-based member resolution is **not implemented** — the `members` field is only written, never expanded.

The BDD spec at `tests/features/monorepo/orchestration.feature` describes extensive monorepo capabilities (member discovery, `--workspace` flag, `--affected` flag, filtering) but **none are implemented**.

### 9. External Crate Options for Recursive Scanning

**`ignore` crate** (from ripgrep ecosystem):
- Parallel directory walking via `WalkBuilder::build_parallel()` with work-stealing
- Built-in `.gitignore`, `.ignore`, `.git/info/exclude` support (on by default)
- Configurable: `hidden()`, `max_depth()`, `follow_links()`, `threads()`
- Callback-based parallel API (not iterator), results collected via `Mutex<Vec<_>>` or channel
- Docs: https://docs.rs/ignore/latest/ignore/

**`rayon` crate**:
- Data-parallelism via `par_iter()` on collected `Vec<Artifact>` for parallel emit
- `ParallelIterator` is not object-safe (`Sized` bound), but iterating *over* trait objects works with `+ Send + Sync`
- `rayon::scope` allows borrowing non-`'static` data in parallel tasks
- Docs: https://docs.rs/rayon/latest/rayon/

**`std::thread::scope`** (Rust 1.63+):
- No external dependency, scoped threads that can borrow from parent stack
- Higher overhead per task (OS threads) — better for few large tasks than many small ones
- Good for coarse-grained parallelism (e.g., one thread per `.claude/` directory found)

## Code References

- `crates/libaipm/src/migrate/mod.rs:158-215` — Orchestrator `migrate()` function, single source dir
- `crates/libaipm/src/migrate/mod.rs:160` — `let source_dir = opts.dir.join(opts.source)` — the single-dir bottleneck
- `crates/libaipm/src/migrate/mod.rs:171-174` — Detector selection hardcoded to `".claude"` match
- `crates/libaipm/src/migrate/mod.rs:200-206` — Sequential emit loop with ordering dependencies
- `crates/libaipm/src/migrate/detector.rs:13-19` — `Detector` trait, single `source_dir` parameter
- `crates/libaipm/src/migrate/detector.rs:23-28` — `claude_detectors()` factory
- `crates/libaipm/src/migrate/skill_detector.rs:18-57` — `SkillDetector::detect()`, stateless, read-only
- `crates/libaipm/src/migrate/command_detector.rs:19-63` — `CommandDetector::detect()`, stateless, read-only
- `crates/libaipm/src/migrate/skill_detector.rs:157-173` — `collect_files_recursive()`, only recursive walker
- `crates/libaipm/src/migrate/emitter.rs:28-34` — `emit_plugin()` signature with `&mut u32` counter
- `crates/libaipm/src/migrate/emitter.rs:205-223` — `resolve_plugin_name()` with mutable counter
- `crates/libaipm/src/migrate/registrar.rs:10-45` — `register_plugins()` read-modify-write
- `crates/libaipm/src/fs.rs:19-33` — `Fs` trait, no `Send`/`Sync` bounds
- `crates/libaipm/src/fs.rs:37` — `Real` struct (unit struct, auto Send+Sync)
- `crates/aipm/src/main.rs:46-58` — CLI `Migrate` command definition
- `crates/libaipm/src/manifest/types.rs:67-78` — `Workspace` struct with `members` globs
- `tests/features/monorepo/orchestration.feature` — Monorepo spec (not implemented)

## Architecture Documentation

### Current Pipeline Architecture

```
CLI (main.rs)
  └─ opts.dir + opts.source = single source_dir
       └─ migrate::migrate()
            ├─ Validate: .ai/ exists, source_dir exists
            ├─ Select detectors: ".claude" → [SkillDetector, CommandDetector]
            ├─ Sequential detect: for det in detectors { det.detect(source_dir, fs) }
            ├─ Collect existing plugin names from .ai/
            ├─ If dry_run: generate report, return
            ├─ Sequential emit: for artifact in all_artifacts { emit_plugin(...) }
            │    └─ Each call depends on previous (rename_counter, known_names)
            └─ Single registrar call: register_plugins(all_names)
```

### Key Architectural Constraints

1. **Fs trait lacks thread-safety bounds** — `&dyn Fs` cannot cross thread boundaries
2. **Detectors are stateless and read-only** — safe to parallelize if Fs is thread-safe
3. **Emitter has ordering deps** — `rename_counter` and `known_names` are sequential state
4. **Registrar is a single batch call** — not a parallelism concern
5. **No recursive directory walking above the source_dir level** — the existing recursive walker (`collect_files_recursive`) only walks *within* a single skill directory

### Parallelization Zones

| Phase | Parallelizable? | Constraint |
|-------|----------------|------------|
| Discovery (find all `.claude/` dirs) | Yes | New code needed; `ignore` crate ideal |
| Detection (run detectors per `.claude/`) | Yes | Fs must be `Send + Sync`; detectors are stateless |
| Emit (create plugin dirs) | Partially | Rename counter is sequential; can parallelize writes if names pre-resolved |
| Register (marketplace.json) | No | Single batch call, not a bottleneck |

## Historical Context (from research/)

- `research/docs/2026-03-23-aipm-migrate-command.md` — Original research for the migrate command implementation
- `specs/2026-03-23-aipm-migrate-command.md` — Spec mentions `Detector` trait extensibility for future folders like `.github/`, `.copilot/` but does not mention recursive scanning
- `tests/features/monorepo/orchestration.feature` — Extensive monorepo spec with workspace members, `--workspace` flag, `--affected` — all unimplemented but establishes the concept of multi-package repos

## Related Research

- `research/docs/2026-03-23-aipm-migrate-command.md` — Initial migrate command research
- `specs/2026-03-23-aipm-migrate-command.md` — Migrate command specification

## Open Questions

1. **Naming conflicts across `.claude/` dirs** — If `packages/auth/.claude/skills/deploy/` and `packages/api/.claude/skills/deploy/` both have a "deploy" skill, how should they be named in the single `.ai/` marketplace? Options: prefix with package path (`auth-deploy`, `api-deploy`), use the existing rename counter, or let the user choose.

2. **Which `.ai/` marketplace to target** — In a monorepo, should all discovered skills go to the root `.ai/`? Or should each subpackage with its own `.ai/` get its own local skills? The current code requires a root-level `.ai/` directory.

3. **Depth limit** — Should recursive scanning have a `--max-depth` flag? Extremely deep repo trees could slow scanning. The `ignore` crate supports `max_depth()`.

4. **Excluded directories** — Beyond `.gitignore`, should `node_modules/`, `target/`, `.git/`, `vendor/`, etc. be hardcoded exclusions? The `ignore` crate handles `.gitignore` natively but explicit excludes may still be wanted.

5. **CLI flag design** — Should this be `--recursive` (opt-in), or should recursive be the default behavior? The `--source` flag currently takes a folder name like `.claude` — does that still make sense when scanning recursively?
