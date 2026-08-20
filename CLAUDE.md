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
cargo +nightly llvm-cov --no-report --doc --branch
cargo +nightly llvm-cov report --doctests --branch \
  --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs|lsp\.rs|libaipm-engine-spec/build\.rs|libaipm-engine-spec/src/bin)'
```

Verify the TOTAL branch column shows ≥ 89%. For HTML or lcov output, append `--html --open` or `--lcov --output-path lcov.info` to the report command.

The `--ignore-filename-regex` pattern is duplicated in `ci.yml` as the `COVERAGE_IGNORE_REGEX` env var — when changing either one, keep the two in sync.

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

The repository uses [GitHub Agentic Workflows](https://githubnext.com/projects/agentics) (`.github/workflows/*.md` compiled via `gh aw compile`) for automated maintenance tasks. One exception: `research-codebase.yml` is intentionally hand-written — see the note below the table.

| Workflow file | Timeout | Schedule | Purpose |
|---|---|---|---|
| `improve-coverage.md` | 45 min | Every 15 min | Finds uncovered branches, writes tests, opens PRs |
| `daily-qa.md` | 45 min | Every 3 h | Validates build, tests, and documentation health |
| `docs-updater.md` | 45 min | Weekdays daily | Syncs docs with recent code changes |
| `update-docs.md` | 45 min | On push to `main` | Updates docs on every merge |
| `build-timings.md` | 45 min | Weekdays daily | Analyzes compilation bottlenecks |
| `reverse-binary-analysis.md` | 120 min | Weekly | Downloads AI engine CLIs, extracts plugin API surface, updates `crates/libaipm-engine-spec/data/engine-api-schema.json`, opens PR when schema changes |
| `research-codebase.yml` † | 30 min | On `research` label applied to issue | Runs Copilot CLI to research the codebase, writes findings to `research/docs/`, posts the generated file as the issue body, and relabels with `spec review` |

† **`research-codebase.yml` is intentionally hand-written and is NOT managed by `gh aw compile`.** It replaced a compiled gh-aw workflow in [#97](https://github.com/TheLarkInn/aipm/pull/97). There is deliberately no `research-codebase.md` source and no `research-codebase.lock.yml` — do not re-add them: `gh aw compile` would emit a second workflow also named "Research Codebase", and both would fire on the same `research` label. Edit the `.yml` directly.

Every workflow in the table also supports manual runs via `workflow_dispatch` (`research-codebase.yml` requires an `issue_number` input when dispatched manually).

### Why different timeouts?

Most maintenance workflows are capped at **45 minutes**: the full agent cycle (nightly toolchain install → build → test → coverage → analysis → PR creation) consistently exceeded shorter limits — `improve-coverage` failed 29+ times at 30 min ([#367](https://github.com/TheLarkInn/aipm/issues/367)), `daily-qa` at 15 min, `docs-updater` at 20 min.

`reverse-binary-analysis` requires **120 minutes** because it installs and analyses multiple engine packages in parallel, which alone can take 30–60 minutes before any writing begins.

**Do not lower these timeouts.** If a workflow still times out, investigate the agent logic — do not reduce the limit.

### Bot merges and push events

`improve-coverage` enables auto-merge on its PRs using the `GH_AW_CI_TRIGGER_TOKEN` PAT (via `safe-outputs.create-pull-request.github-token`), so merge commits on `main` are attributed to a real user and emit `push` events that trigger `ci.yml` and `update-docs`. Merges made with the default `GITHUB_TOKEN` have their events suppressed by GitHub — this silently disabled post-merge CI for ~3 months ([#1387](https://github.com/TheLarkInn/aipm/issues/1387)).

The manual `ci: trigger checks` empty-commit workaround is **obsolete** — do not push empty commits to trigger CI on `main`. If post-merge CI stops running again, check which token enabled auto-merge on the merged PR.

### A green run is not a healthy run

A gh-aw workflow that emits only `noop` outputs still exits `success` — the run log literally says "Agent succeeded with only noop outputs - this is not a failure". A workflow can therefore be livelocked while the dashboard shows green for weeks: `improve-coverage` parked indefinitely on an unmergeable PR exactly this way ([#1391](https://github.com/TheLarkInn/aipm/issues/1391)). When auditing a workflow, check what it *did* — issues/PRs created, comments posted — not just its conclusion, and treat repeated identical `noop` cycles as a failure signal.

### Modifying workflow files

After editing any `.github/workflows/<name>.md`, recompile its lock file:

```bash
gh aw compile <name>   # e.g. gh aw compile improve-coverage
```

Never hand-edit a `.lock.yml`: commit both the `.md` source and the regenerated `.lock.yml` together. The compiled lock file is the canonical version GitHub Actions runs.

#### Keep every lock file on the same compiler version

**All lock files must be compiled with the same `gh aw` version.** The single source of truth for the pinned version is the `GH_AW_VERSION` env var in `.github/workflows/ci.yml` — bump it there when upgrading and recompile every lock with that version. Each lock file's `# gh-aw-metadata:` header also records the version it was compiled with as `compiler_version`. Install the pinned version with `gh extension install github/gh-aw --pin <version>` before recompiling (check yours with `gh aw version`) — do not float to `latest`. The `aw-lock-drift` job in `ci.yml` installs the pinned version, fails on mixed or pin-mismatched `compiler_version` headers, runs `gh aw compile`, and fails the build on any resulting diff or untracked files under `.github/workflows` / `.github/aw` — out-of-date locks cannot slip through on the honour system ([#1390](https://github.com/TheLarkInn/aipm/issues/1390)).

Recompiling a single workflow with a newer extension than the others introduces *compiler version skew*. Each lock file embeds a pinned [`gh-aw-firewall`](https://github.com/githubnext/gh-aw-firewall) (AWF) release, so a skewed lock ends up on a different AWF pin than its siblings. Upstream deletes old AWF releases, and when that happens the `Install AWF binary` step dies with `curl: (22) ... 404` and the workflow fails 100% of the time — which is exactly how `reverse-binary-analysis` (pinned to the deleted `v0.25.28`) silently failed every week for over two months ([#1388](https://github.com/TheLarkInn/aipm/issues/1388)).

When upgrading the extension, run a bare `gh aw compile` to recompile **all** workflows at once, and confirm the pins agree before committing:

```bash
grep -ho 'install_awf_binary\.sh" v[0-9.]*' .github/workflows/*.lock.yml | awk '{print $2}' | sort -u
```

That must print exactly one version, and it must resolve upstream:

```bash
gh api repos/githubnext/gh-aw-firewall/releases/tags/<version> --jq .tag_name
```

#### Lock files are generated — `.gitattributes` is deliberately minimal

`.gitattributes` carries exactly one rule for the compiled locks:

```
.github/workflows/*.lock.yml linguist-generated=true
```

`linguist-generated=true` keeps the generated locks out of PR diffs and repo language stats. The rule **deliberately does not** set `merge=ours` ([#1392](https://github.com/TheLarkInn/aipm/issues/1392)): `merge=ours` only works when every contributor configures the `ours` merge driver locally (`git config merge.ours.driver true`), so it was inert for most clones, and when it *did* fire it silently resolved lock conflicts to the local side instead of regenerating from the `.md` source — precisely the drift this section exists to prevent. A conflict in a lock file must be resolved by recompiling (`gh aw compile`), never by keeping one side, so a loud conflict is the desired behaviour. The follow-up CI guard in [#1390](https://github.com/TheLarkInn/aipm/issues/1390) makes out-of-date locks fail the build outright.

#### `agentics-maintenance.yml` is generated and adopted

`gh aw compile` also emits a generated `.github/workflows/agentics-maintenance.yml` (no `.md` source). It is **adopted deliberately** ([#1392](https://github.com/TheLarkInn/aipm/issues/1392)), not committed by accident. It exists because `daily-qa.md` uses a `create-discussion` safe output, which the compiler gives a default 7-day `expires` — the maintenance workflow's scheduled jobs close those expired discussions (and any expired issues/PRs/cache) so they do not accumulate. It runs daily at 00:37 UTC with a top-level `permissions: {}` and least-privilege per-job permissions, and also exposes manual `workflow_dispatch` maintenance operations. This does **not** overlap with `stale-bot-prs.yml`, which closes *idle* bot-authored PRs on a separate stale/grace policy rather than *expired* safe outputs. To suppress the generated file, set `{"maintenance": false}` in `.github/workflows/aw.json`; do not hand-edit the file — it is regenerated by `gh aw compile`.

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
