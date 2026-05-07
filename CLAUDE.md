# CLAUDE.md — Project Rules for AI Agents

## Lint Policy (STRICTLY ENFORCED — compiler will reject violations)

All lints are configured in `Cargo.toml` under `[workspace.lints]`. The key rules:

1. **NEVER add `#[allow(...)]`, `#[expect(...)]`, or `#![allow(...)]` attributes.** The `allow_attributes` lint is set to `warn` — the CI `-D warnings` flag treats this as an error, so hand-written lint suppressions are rejected in practice. Derive macros (serde, thiserror) are permitted to emit internal `#[allow]`.

2. **NEVER use `.unwrap()`, `.expect()`, `panic!()`, `todo!()`** — these are all `deny` (`unwrap_used`, `expect_used`, `panic`, `todo`). Use proper error handling with `Result`/`Option` combinators, `?` operator, or `if let`/`match`. Also avoid `unimplemented!()` and `unreachable!()` — they are not explicitly denied but expand to panics and should not appear in production code.

3. **NEVER use `println!()`, `eprintln!()`, `print!()`, `eprint!()`** — these are `deny`. Use `std::io::Write` with `write!()`/`writeln!()` for output, or a logging framework.

4. **NEVER use `dbg!()`** — `deny`. Remove all debug macros before committing.

5. **NEVER use `unsafe`** — `forbid`. No unsafe code anywhere.

6. **NEVER use `std::process::exit()`** — `deny`. Return from main instead.

7. **NEVER use `.unwrap()` inside functions returning `Result`** — `deny` via `unwrap_in_result`.

8. **Use `.get()` instead of `[]` indexing** where possible — `indexing_slicing` is `warn`.

9. **Use `create_dir_all` instead of `create_dir`** — `create_dir` is `warn`.

## If a lint blocks you

Do NOT suppress it. Instead:
- **Fix the code** to satisfy the lint
- If a lint is genuinely wrong for a specific case, raise it — the `Cargo.toml` config is the single source of truth, not inline attributes

## Build Commands

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

All four must pass with zero warnings before any commit.

## Coverage Commands (MANDATORY before pushing)

89% branch coverage is required. Prereqs: `rustup toolchain install nightly && rustup component add llvm-tools-preview --toolchain nightly && cargo install cargo-llvm-cov`

```bash
cargo +nightly llvm-cov clean --workspace
cargo +nightly llvm-cov --no-report --workspace --branch
cargo +nightly llvm-cov --no-report --doc
cargo +nightly llvm-cov report --doctests --branch \
  --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs|lsp\.rs|libaipm-engine-spec/build\.rs|libaipm-engine-spec/src/bin)'
```

Verify the TOTAL branch column shows ≥ 89%. For HTML or lcov output, append `--html --open` or `--lcov --output-path lcov.info` to the report command.

## Copilot Coding Agent Setup

The file `.github/workflows/copilot-setup-steps.yml` defines the pre-build environment for the [GitHub Copilot coding agent](https://docs.github.com/en/copilot/using-github-copilot/using-claude-sonnet-in-github-copilot). It runs before every agent task and installs all toolchain prerequisites so the sandbox does not need network access during the actual build:

| Step | Purpose |
|---|---|
| `dtolnay/rust-toolchain@stable` | Installs `clippy` and `rustfmt` |
| `dtolnay/rust-toolchain@nightly` | Installs `llvm-tools-preview` for coverage |
| `apt-get install libgit2-dev libssl-dev pkg-config` | Native system libraries required by `git2` and OpenSSL crates |
| `Swatinem/rust-cache@v2` | Caches the Cargo registry and build artefacts across runs |
| `cargo fetch --locked` | **Pre-fetches all Cargo dependencies** so they are available in the offline sandbox environment |

> **Why `cargo fetch --locked`?** The Copilot agent sandbox has restricted network access after the setup phase. Without this step the build fails because crates cannot be downloaded during `cargo build`. Pre-fetching under `--locked` also guarantees the exact dependency graph recorded in `Cargo.lock` is used — no accidental updates. This step was added to fix the [#700](https://github.com/TheLarkInn/aipm/pull/700) sandbox build failures.

Do **not** remove or weaken `CARGO_NET_RETRY` (currently `10`) or the `--locked` flag — both are necessary for reliability in flaky network conditions.

## Agentic Workflows

The repository uses [GitHub Agentic Workflows](https://githubnext.com/projects/agentics) (`.github/workflows/*.md` compiled via `gh aw compile`) for automated maintenance tasks.

| Workflow file | Timeout | Schedule | Purpose |
|---|---|---|---|
| `improve-coverage.md` | 45 min | Every 15 min | Finds uncovered branches, writes tests, opens PRs |
| `daily-qa.md` | 45 min | Every 3 h | Validates build, tests, and documentation health |
| `docs-updater.md` | 45 min | Weekdays daily | Syncs docs with recent code changes |
| `update-docs.md` | 45 min | On push to `main` | Updates docs on every merge |
| `build-timings.md` | 45 min | Weekdays daily | Analyzes compilation bottlenecks |
| `reverse-binary-analysis.md` | 120 min | Weekly | Downloads AI engine CLIs, extracts plugin API surface, updates `crates/libaipm-engine-spec/data/engine-api-schema.json`, opens PR when schema changes |
| `research-codebase.md` | 30 min | On `research` label applied to issue | Runs Copilot CLI to research the codebase, posts findings as the issue body, and relabels with `spec review` |

### Why different timeouts?

Most maintenance workflows are capped at **45 minutes**: the full agent cycle (nightly toolchain install → build → test → coverage → analysis → PR creation) consistently exceeded shorter limits — `improve-coverage` failed 29+ times at 30 min ([#367](https://github.com/TheLarkInn/aipm/issues/367)), `daily-qa` at 15 min, `docs-updater` at 20 min.

`reverse-binary-analysis` requires **120 minutes** because it installs and analyses multiple engine packages in parallel, which alone can take 30–60 minutes before any writing begins.

**Do not lower these timeouts.** If a workflow still times out, investigate the agent logic — do not reduce the limit.

### Modifying workflow files

After editing any `.github/workflows/<name>.md`, recompile its lock file:

```bash
gh aw compile <name>   # e.g. gh aw compile improve-coverage
```

Commit both the `.md` source and the regenerated `.lock.yml` together. The compiled lock file is the canonical version GitHub Actions runs.

## Project Structure

- `Cargo.toml` — workspace root, lint configuration
- `rustfmt.toml` — formatting rules (100 char width, Unix newlines)
- `clippy.toml` — clippy thresholds (complexity, stack size, test exemptions)
- `crates/aipm/` — consumer CLI binary (`init`, `install`, `update`, `uninstall`, `link`, `unlink`, `list`, `lint`, `migrate`, `make`, `pack`, `lsp`)
- `crates/libaipm/` — core library (manifest, validation, migration, scaffolding, lint, install, link, resolve)
- `crates/libaipm-engine-spec/` — engine-API source-of-truth: canonical Rust types in `src/types.rs`, `data/engine-api-schema.json` (weekly-regenerated by `reverse-binary-analysis`), `build.rs` validates the data and emits typed const tables (`Engine`, `EngineSet`, `ENGINES`, `VALID_TOOLS`, `TOOL_COMPATIBILITY`, `FEATURES_BY_ENGINE`, `HOOK_EVENTS_BY_ENGINE`, `paths`, `constraints`) into `OUT_DIR/engine_data.rs`
- `schemas/engine-api.schema.json` — JSON Schema (draft 2020-12) derived from `libaipm-engine-spec`'s Rust types via `bin/export-schema`
- `vscode-aipm/` — VS Code extension (TypeScript; lint diagnostics, completions, hover for `aipm.toml`; not a Cargo workspace member)
- `specs/` — technical design documents
- `tests/features/` — cucumber-rs BDD feature files
- `research/` — research documents and feature tracking

## Schema export (libaipm-engine-spec)

The `crates/libaipm-engine-spec/` crate's Rust types in `src/types.rs` are the canonical shape for the engine-API schema. The committed `schemas/engine-api.schema.json` is a derived artefact (JSON Schema 2020-12) that mirrors those types. `build.rs` validates `data/engine-api-schema.json` against the committed meta-schema on every build, so any drift between the data file and the Rust types is a build failure.

When you change `src/types.rs` in a way that affects the on-the-wire shape:

```bash
cargo run -p libaipm-engine-spec --bin export-schema
```

This regenerates `schemas/engine-api.schema.json`. Commit the regenerated schema with the type change. The `tests/schema_export_drift.rs` integration test fails if the committed schema differs semantically from what schemars would produce now.

If the change is **breaking** (renames, removed fields, type changes that existing data files won't deserialize through), bump `META_SCHEMA_VERSION` in `src/types.rs`. The `data/engine-api-schema.json` file's `meta_schema_version` field must equal the constant — `build.rs` enforces this. Bumping the version forces the next reverse-binary-analysis run to emit a data file with the new shape.

`build.rs` and `src/bin/export-schema.rs` are excluded from the coverage `--ignore-filename-regex` pattern (they only run during `cargo build` / on demand, not under normal test execution).
