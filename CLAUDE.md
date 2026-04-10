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

Coverage uses nightly Rust for branch-level instrumentation. The coverage check
is a **correctness gate** — LLM-generated code must hit 89% branch coverage.

```bash
# Clean prior coverage data to ensure a fresh run (matches CI behavior)
cargo +nightly llvm-cov clean --workspace

# 1) Collect workspace test coverage (no report yet)
cargo +nightly llvm-cov --no-report --workspace --branch

# 2) Collect doctest coverage (no report yet)
cargo +nightly llvm-cov --no-report --doc

# 3) Generate report — verify TOTAL line branch column shows >= 89%
cargo +nightly llvm-cov report --doctests --branch \
  --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs|lsp\.rs)'

# HTML report (visual inspection; uses merged tests + doctests)
cargo +nightly llvm-cov report --workspace --branch --doctests \
  --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs|lsp\.rs)' \
  --html --open

# lcov for VS Code Coverage Gutters extension (uses merged tests + doctests)
cargo +nightly llvm-cov report --workspace --branch --doctests \
  --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs|lsp\.rs)' \
  --lcov --output-path lcov.info
```

All coverage commands require the nightly toolchain and llvm-tools-preview:
```bash
rustup toolchain install nightly
rustup component add llvm-tools-preview --toolchain nightly
cargo install cargo-llvm-cov
```

## Agentic Workflows

The repository uses [GitHub Agentic Workflows](https://githubnext.com/projects/agentics) (`.github/workflows/*.md` compiled via `gh aw compile`) for automated maintenance tasks. All workflows are set to `timeout-minutes: 45`.

| Workflow file | Schedule | Purpose |
|---|---|---|
| `improve-coverage.md` | Every 15 min | Finds uncovered branches, writes tests, opens PRs |
| `daily-qa.md` | Every 3 h | Validates build, tests, and documentation health |
| `docs-updater.md` | Weekdays daily | Syncs docs with recent code changes |
| `update-docs.md` | On push to `main` | Updates docs on every merge |
| `build-timings.md` | Weekdays daily | Analyzes compilation bottlenecks |

### Why 45 minutes?

The full agent cycle (Rust nightly toolchain install → build → test → coverage instrumentation → analysis → PR creation) consistently exceeded shorter timeouts. Specifically:

- `improve-coverage` was killed at 30 min with 29+ repeated failures ([issue #367](https://github.com/TheLarkInn/aipm/issues/367))
- `daily-qa` was killed at 15 min
- `docs-updater` was killed at the default 20 min

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
- `crates/aipm/` — consumer CLI binary (`init`, `install`, `update`, `uninstall`, `link`, `unlink`, `list`, `lint`, `migrate`)
- `crates/aipm-pack/` — author CLI binary (`init`)
- `crates/libaipm/` — core library (manifest, validation, migration, scaffolding, lint, install, link, resolve)
- `specs/` — technical design documents
- `tests/features/` — cucumber-rs BDD feature files
- `research/` — research documents and feature tracking
