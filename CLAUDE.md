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
  --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs|lsp\.rs)'
```

Verify the TOTAL branch column shows ≥ 89%. For HTML or lcov output, append `--html --open` or `--lcov --output-path lcov.info` to the report command.

## Agentic Workflows

The repository uses [GitHub Agentic Workflows](https://githubnext.com/projects/agentics) (`.github/workflows/*.md` compiled via `gh aw compile`) for automated maintenance tasks.

| Workflow file | Timeout | Schedule | Purpose |
|---|---|---|---|
| `improve-coverage.md` | 45 min | Every 15 min | Finds uncovered branches, writes tests, opens PRs |
| `daily-qa.md` | 45 min | Every 3 h | Validates build, tests, and documentation health |
| `docs-updater.md` | 45 min | Weekdays daily | Syncs docs with recent code changes |
| `update-docs.md` | 45 min | On push to `main` | Updates docs on every merge |
| `build-timings.md` | 45 min | Weekdays daily | Analyzes compilation bottlenecks |
| `reverse-binary-analysis.md` | 120 min | Weekly | Downloads AI engine CLIs, extracts plugin API surface, updates `research/engine-api-schema.json`, opens PR when schema changes |
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
- `specs/` — technical design documents
- `tests/features/` — cucumber-rs BDD feature files
- `research/` — research documents and feature tracking
