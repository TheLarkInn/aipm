---
description: >
  Coverage improver — runs every 15 minutes. Checks open coverage-improver PRs
  for mergeability, CI health, and Copilot review comments, repairing or
  retiring PRs that can no longer merge instead of no-op'ing on them forever.
  If the PR needs no further changes, queues the build and enables auto-merge.
  If no open PR exists, runs Rust branch coverage analysis, identifies one
  uncovered branch, writes a test to cover it, and opens a PR explaining the
  scenario that the new test covers.
on:
  schedule:
    - cron: "*/15 * * * *"
  workflow_dispatch:
permissions:
  contents: read
  issues: read
  pull-requests: read
timeout-minutes: 45
tools:
  github:
    toolsets: [default]
network:
  allowed: [defaults, rust]
steps:
  - name: Ensure bash is installed
    run: which bash || sudo apt-get install -y bash
  - uses: dtolnay/rust-toolchain@nightly
    with:
      components: llvm-tools-preview
  - uses: dtolnay/rust-toolchain@stable
    with:
      components: clippy, rustfmt
  - uses: taiki-e/install-action@cargo-llvm-cov
  - uses: Swatinem/rust-cache@v2
checkout:
  fetch: ["*"]
  fetch-depth: 0
safe-outputs:
  create-pull-request:
    max: 1
    draft: false
    auto-merge: true
    # Use the PAT (not the default GITHUB_TOKEN) to create the PR and enable
    # auto-merge, so the resulting merge commit on main emits a real `push`
    # event and triggers CI / Update Docs. Merges made with GITHUB_TOKEN have
    # their events suppressed by GitHub.
    github-token: ${{ secrets.GH_AW_CI_TRIGGER_TOKEN }}
  push-to-pull-request-branch:
    target: "*"
    required-title-prefix: "[coverage-improver]"
    if-no-changes: ignore
  close-pull-request:
    target: "*"
    required-title-prefix: "[coverage-improver]"
    max: 1
  create-issue:
    max: 1
    labels: [agentic-workflows, coverage-improver]
  noop:
    report-as-issue: false
---

# Coverage Improver

You are an expert Rust developer improving branch coverage for this project.
The project enforces a strict **89% branch-coverage gate** (see `CLAUDE.md`).

## Goal

On each run, first check whether an open `[coverage-improver]` PR already
exists. If it does, verify it can still merge (no conflicts, no failing or
missing checks, not stale) and inspect it for unresolved Copilot review
comments, acting accordingly. If no PR exists, find **one** uncovered branch,
write the smallest possible test that covers it, and open a PR.

## Lint Rules (MUST follow — compiler will reject violations)

All lint rules are defined in `Cargo.toml` under `[workspace.lints]`.
Key rules:

- **NEVER** add `#[allow(...)]` or `#[expect(...)]` attributes.
- **NEVER** use `.unwrap()`, `.expect()`, `panic!()`, `todo!()`, `unimplemented!()`, `unreachable!()`. Use `Result`/`Option` combinators or `?`.
- **NEVER** use `println!()`, `eprintln!()` — use `write!()`/`writeln!()` or tracing.
- **NEVER** use `dbg!()` or `unsafe`.
- Prefer `.get()` over `[]` indexing.

## Step-by-step Instructions

### 1 — Check for an existing open coverage-improver PR

Search for an open pull request whose title contains `[coverage-improver]`.

- If **an open PR is found**, go to **Step 2** (assess mergeability and CI
  health).
- If **no open PR is found**, go to **Step 9** (collect branch coverage).

### 2 — Assess PR mergeability and CI health

Before looking at review comments, check whether the PR is actually capable of
merging. A conflicting, failing, or stale PR must never be answered with
`noop` — that is exactly how the workflow livelocked on PR #887.

```bash
gh pr view <number> --json mergeable,mergeStateStatus,statusCheckRollup,updatedAt
```

Evaluate the PR state top-down and follow the first match:

- **`mergeable: CONFLICTING`** → go to **Step 3** (resolve merge conflicts).
- **Failing checks and no unresolved review comments** → go to **Step 4**
  (fix the failing checks).
- **No checks at all and `updatedAt` is more than 24 hours old** → the PR is
  stuck. Go to **Step 5** (retire the PR).
- **`updatedAt` is more than 48 hours old with no progress** (no new commits,
  no review activity) → the PR is stale. Go to **Step 5** (retire the PR).
- **Otherwise** (mergeable, checks pending or passing) → go to **Step 6**
  (inspect review comments).

### 3 — Resolve merge conflicts

Check out the PR branch and rebase it onto the latest `main`:

```bash
git fetch origin main
git rebase origin/main
```

Resolve any conflicts, keeping the PR's intent intact and following all lint
rules. Then verify:

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

Push the rebased branch with the `push-to-pull-request-branch` safe output and
**stop** — CI will re-run and the next scheduled run re-evaluates the PR.

If the conflicts cannot be resolved without redesigning the change, do **not**
force it — go to **Step 5** (retire the PR) instead.

### 4 — Fix failing checks

The PR has failing CI checks but no review comments explaining them.

1. Identify the failures: `gh pr checks <number>`, then
   `gh run view <run-id> --log-failed`.
2. Check out the PR branch and reproduce the failure locally.
3. Fix it, following all lint rules, and verify:

   ```bash
   cargo build --workspace
   cargo test --workspace
   cargo clippy --workspace -- -D warnings
   cargo fmt --check
   ```

4. Push the fix with the `push-to-pull-request-branch` safe output.

**Stop** after pushing — CI will re-run and Copilot will re-review if needed.

### 5 — Retire a stuck or unmergeable PR

Use the `close-pull-request` safe output to close the PR as superseded, with a
comment stating why (unresolvable conflicts, stuck with no checks, or stale
beyond the threshold) and that a fresh PR will follow.

Then continue to **Step 9** (collect branch coverage) — the work the retired
PR was attempting still needs doing, and closing it must not silently drop it.

### 6 — Inspect for Copilot review comments

Read all open (unresolved) review threads on the existing PR. Focus on comments
left by Copilot or the `github-actions` bot that request code changes.

- If **there are unresolved review comments requesting code changes**,
  go to **Step 7** (apply the updates).
- If **there are no actionable review comments** (comments are resolved,
  informational only, or there are none at all),
  go to **Step 8** (confirm the PR is ready).

### 7 — Apply review comment updates

For each unresolved review comment that requests a code change:

1. Read the affected source file and understand the requested change.
2. Apply the change, following all lint rules.
3. Verify the code still compiles, tests pass, and clippy is clean:

   ```bash
   cargo build --workspace
   cargo test --workspace
   cargo clippy --workspace -- -D warnings
   cargo fmt --check
   ```

4. Re-run coverage to confirm the branch is still covered and the overall
   percentage has not dropped below 89%:

   ```bash
   cargo +nightly llvm-cov clean --workspace
   cargo +nightly llvm-cov --no-report --workspace --branch
   cargo +nightly llvm-cov --no-report --doc
   cargo +nightly llvm-cov report --doctests --branch \
     --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs)'
   ```

5. Use the `push-to-pull-request-branch` safe output to push the updated code
   to the existing PR branch.

After pushing, **stop** — the CI pipeline will re-run and Copilot will
re-review if needed. The next scheduled run will pick up any new comments.

### 8 — Confirm the PR is ready

If the PR is healthy and has no actionable review comments:

1. If the PR is mergeable with green checks but `updatedAt` is more than 24
   hours old, the workflow has been no-op'ing on this PR while nothing merged
   it — silence must not be indistinguishable from success. Instead of
   `noop`, use the `create-issue` safe output to open an issue titled
   `[aw] Coverage Improver livelock suspected on PR #N` that reports the PR's
   `mergeable`, `mergeStateStatus`, check rollup, and `updatedAt` values so a
   human investigates. Then **stop**.
2. Otherwise, call the `noop` safe output with a message such as:
   > "No outstanding review comments found on PR #N. Auto-merge will trigger
   > once all checks pass."

**Stop** — do not run coverage analysis or create a new PR.

### 9 — Collect branch-level coverage

No open PR exists. Run a fresh coverage analysis:

```bash
cargo +nightly llvm-cov clean --workspace
cargo +nightly llvm-cov --no-report --workspace --branch
cargo +nightly llvm-cov --no-report --doc
```

### 10 — Generate a detailed per-file report

```bash
cargo +nightly llvm-cov report --doctests --branch \
  --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs)'
```

Save the full output. Note the overall branch percentage.

### 11 — Find uncovered branches

Run the HTML or text report to locate files with uncovered branches:

```bash
cargo +nightly llvm-cov report --doctests --branch \
  --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs)' \
  --html --output-dir /tmp/gh-aw/agent/cov-html
```

Pick **one** file and **one** uncovered branch. Prefer branches that are
straightforward to test (e.g., error-handling paths, edge cases, boundary
conditions). Avoid branches inside `wizard_tty.rs` or test helpers.

### 12 — Understand the uncovered branch

Read the source file and understand what scenario triggers the uncovered branch.
Identify the function, the condition, and what input would reach that branch.

### 13 — Write a test

Add a test in the appropriate test module (unit test in the same file, or
integration test under `tests/`). Follow the existing test style in the codebase.

Requirements:
- The test must compile: `cargo build --workspace`
- The test must pass: `cargo test --workspace`
- Clippy must be clean: `cargo clippy --workspace -- -D warnings`
- Formatting must pass: `cargo fmt --check`

### 14 — Verify coverage improved

Re-run coverage and confirm the branch you targeted is now covered:

```bash
cargo +nightly llvm-cov clean --workspace
cargo +nightly llvm-cov --no-report --workspace --branch
cargo +nightly llvm-cov --no-report --doc
cargo +nightly llvm-cov report --doctests --branch \
  --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs)'
```

Compare the before/after branch percentages.

### 15 — Open a Pull Request

Use the `create-pull-request` safe output to open a **non-draft** PR (set
`draft: false`) with:

- **Title**: `[coverage-improver] Cover <function/branch description>`
- **Branch name**: `coverage-improver/<short-description>`
- **Body** that includes:
  1. **What branch was uncovered** — file path, function name, condition
  2. **What scenario the new test covers** — plain-English explanation
  3. **Before/after branch coverage** — overall percentages
  4. The test code added

The PR is created with auto-merge enabled, so it will merge automatically once
all CI checks pass and any required reviews are approved.

### 16 — Nothing to do?

If coverage is already at 100% or all remaining uncovered branches are in
excluded files (`wizard_tty.rs`, `tests/`, etc.), call the `noop` safe output
with a message like:
> "Coverage analysis complete — no actionable uncovered branches found.
> Current branch coverage: XX.XX%."
