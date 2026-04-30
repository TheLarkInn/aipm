# Contributing to aipm

Welcome! This guide covers how to set up a development environment, build and test the project, and submit changes. For the rules that AI coding agents must follow when working on this codebase, see [CLAUDE.md](../../CLAUDE.md).

## Prerequisites

- **Rust stable** — `rustup toolchain install stable`
- **Rust nightly** — `rustup toolchain install nightly` (required for coverage)
- **System libraries** (Linux):

  ```bash
  sudo apt-get install libgit2-dev libssl-dev pkg-config
  ```

- **`cargo-llvm-cov`** (for coverage):

  ```bash
  rustup component add llvm-tools-preview --toolchain nightly
  cargo install cargo-llvm-cov
  ```

## Build

```bash
cargo build --workspace
```

## Test

```bash
cargo test --workspace
```

## Lint

```bash
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

All four commands (`build`, `test`, `clippy`, `fmt`) must pass with zero warnings. CI enforces this with `-D warnings`.

## Coverage

89% branch coverage is required before opening a pull request:

```bash
cargo +nightly llvm-cov clean --workspace
cargo +nightly llvm-cov --no-report --workspace --branch
cargo +nightly llvm-cov --no-report --doc
cargo +nightly llvm-cov report --doctests --branch \
  --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs|lsp\.rs)'
```

Verify the **TOTAL** branch column shows ≥ 89%.

## Lint policy highlights

The project enforces a strict set of Clippy lints (configured in `Cargo.toml`):

- **No `#[allow(...)]` attributes** — the `allow_attributes` lint is `warn`; CI's `-D warnings` makes it an error.
- **No `.unwrap()` / `.expect()` / `panic!()`** — use `Result`/`Option` combinators or `?`.
- **No `println!()` / `eprintln!()`** — use `std::io::Write` or a logging framework.
- **No `unsafe`** — `forbid`.
- **No `std::process::exit()`** — return from `main`.

Fix violations in code rather than suppressing them with attributes. See [CLAUDE.md](../../CLAUDE.md) for the full list.

## Agentic workflows

Several automated workflows run on this repository:

| Workflow | Schedule | Purpose |
|----------|----------|---------|
| `improve-coverage.md` | Every 15 min | Finds uncovered branches and opens PRs with tests |
| `daily-qa.md` | Every 3 h | Validates build, tests, and documentation health |
| `docs-updater.md` | Weekdays daily | Syncs docs with recent code changes |
| `update-docs.md` | On push to `main` | Updates docs on every merge |
| `build-timings.md` | Weekdays daily | Analyzes compilation bottlenecks |

Workflow sources are the `.github/workflows/<name>.md` files. After editing one, regenerate its lock file:

```bash
gh aw compile <name>   # e.g. gh aw compile improve-coverage
```

Commit both the `.md` source and the regenerated `.lock.yml` together.

## Submitting changes

1. Fork the repository and create a branch from `main`.
2. Run build, test, lint, and coverage checks (all must pass).
3. Open a pull request with a clear description of what changed and why.
4. Use [conventional commits](https://www.conventionalcommits.org/) for commit messages:
   - `feat:` new feature
   - `fix:` bug fix
   - `docs:` documentation only
   - `chore:` maintenance
   - `test:` adding or fixing tests

## Release process

See [RELEASING.md](../../RELEASING.md) for the full runbook.

## See also

- [CLAUDE.md](../../CLAUDE.md) — complete rules for AI coding agents
- [Project structure](../../README.md#project-structure) — overview of crates and directories
- [Roadmap](../../README.md#roadmap) — planned features and their status
