---
description: >
  Coverage improver — runs every 15 minutes. Checks open coverage-improver PRs
  for Copilot review comments, merge conflicts, failing or missing checks, and
  staleness; repairs or replaces unmergeable PRs instead of no-oping on them,
  and escalates repeated no-op cycles to an issue so a livelock cannot pass as
  success. If the PR needs no further changes, queues the build and enables
  auto-merge. If no open PR exists, runs Rust branch coverage analysis,
  identifies one uncovered branch, writes a test to cover it, and opens a PR
  explaining the scenario that the new test covers.
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
    deduplicate-by-title: true
  noop:
    report-as-issue: false
---

# Coverage Improver

You are an expert Rust developer improving branch coverage for this project.
The project enforces a strict **89% branch-coverage gate** (see `CLAUDE.md`).

## Goal

On each run, first check whether an open `[coverage-improver]` PR already
exists. If it does, assess whether it can actually merge — unresolved Copilot
review comments, merge conflicts, failing or missing checks, and staleness all
require action, **not** a `noop`. Repair the PR, or close it as superseded and
start fresh. Only `noop` when the PR is healthy and waiting on CI or review,
and escalate to an issue when the same PR keeps producing `noop` runs. If no
PR exists, find **one** uncovered branch, write the smallest possible test
that covers it, and open a PR.

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

- If **an open PR is found**, go to **Step 2** (assess PR health).
- If **no open PR is found**, go to **Step 6** (create a new PR).

### 2 — Assess PR health and mergeability

A `noop` on a PR that can never merge is what livelocked this workflow
(conflicting PR #887 stalled it indefinitely while every run reported
`success`). Never `noop` on an unmergeable or stuck PR.

1. Check mergeability, check status, and last activity:

   ```bash
   gh pr view <number> --json mergeable,mergeStateStatus,statusCheckRollup,updatedAt
   ```

2. Read all open (unresolved) review threads on the PR. Focus on comments
   left by Copilot or the `github-actions` bot that request code changes.

Decide:

- **Unresolved review comments requesting code changes** → go to **Step 4**
  (apply the updates).
- **`mergeable: CONFLICTING`** → go to **Step 3** (repair or replace).
  Do **not** `noop`.
- **Failing status checks with no actionable review comments** → go to
  **Step 3** (fix the failures). Do **not** `noop`.
- **No status checks at all** and `updatedAt` is older than **6 hours** → the
  PR is stuck; go to **Step 3**. Do **not** `noop` indefinitely.
- **`updatedAt` older than 48 hours** with no progress (no new commits,
  comments, or check movement) → close as superseded in **Step 3** and start
  fresh. Do **not** `noop`.
- Otherwise (mergeable, checks green or normally pending, no actionable
  review comments) → go to **Step 5** (confirm the PR is ready).

### 3 — Repair or replace an unmergeable or stuck PR

**Prefer repair when the branch is salvageable** (conflicts are small, or the
check failure is fixable from the PR's diff):

1. Check out the PR branch and rebase it onto the latest `main` (or merge
   `main` into it), resolving any conflicts. Follow all lint rules.
2. Investigate failing checks (`gh pr checks <number>` and the run logs) and
   fix the underlying failures.
3. Verify the code compiles, tests pass, clippy is clean, and coverage stays
   at or above 89%:

   ```bash
   cargo build --workspace
   cargo test --workspace
   cargo clippy --workspace -- -D warnings
   cargo fmt --check
   cargo +nightly llvm-cov clean --workspace
   cargo +nightly llvm-cov --no-report --workspace --branch
   cargo +nightly llvm-cov --no-report --doc
   cargo +nightly llvm-cov report --doctests --branch \
     --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs)'
   ```

4. Use the `push-to-pull-request-branch` safe output to push the repair to
   the existing PR branch, then **stop**.

**If the branch is not salvageable** (conflicts too large to resolve safely,
the failure is not fixable from the PR's diff, or the PR is stale beyond
repair):

1. Call the `close-pull-request` safe output targeting the PR, with a comment
   explaining it is closed as superseded (conflicting, stuck, or stale) and
   that a fresh coverage PR will be opened.
2. Continue to **Step 6** to run a fresh coverage analysis and open a new PR.

Never end this step with `noop` — a `noop` here is the livelock this
workflow must not re-enter.

### 4 — Apply review comment updates

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

### 5 — Confirm the PR is ready

Reach this step only when the existing PR is healthy: mergeable, checks green
or normally pending, and no actionable review comments.

1. **Livelock guard** — before nooping, verify this is not another silent
   no-op cycle. List recent runs of this workflow and count how many have
   completed since the PR's `updatedAt` timestamp from Step 2:

   ```bash
   gh run list --workflow "Coverage Improver" --status completed --limit 30 \
     --json databaseId,conclusion,createdAt
   ```

   If **more than 8 consecutive runs** (≈2 hours at the 15-minute cadence)
   completed since the PR last changed and its checks are still not complete,
   the workflow is spinning on a stuck PR. Do **not** call `noop`. Instead
   call `create-issue` with exactly this title so repeat runs deduplicate:

   > `[coverage-improver] Livelock suspected: repeated no-op runs on PR #N`

   …and a body describing the PR's mergeability, check status, and how long
   it has been stuck. **Stop.**

2. Otherwise call the `noop` safe output with a message such as:
   > "No outstanding review comments found on PR #N. Auto-merge will trigger once
   > all checks pass."

**Stop** — do not run coverage analysis or create a new PR.

### 6 — Collect branch-level coverage

No open PR exists. Run a fresh coverage analysis:

```bash
cargo +nightly llvm-cov clean --workspace
cargo +nightly llvm-cov --no-report --workspace --branch
cargo +nightly llvm-cov --no-report --doc
```

### 7 — Generate a detailed per-file report

```bash
cargo +nightly llvm-cov report --doctests --branch \
  --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs)'
```

Save the full output. Note the overall branch percentage.

### 8 — Find uncovered branches

Run the HTML or text report to locate files with uncovered branches:

```bash
cargo +nightly llvm-cov report --doctests --branch \
  --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs)' \
  --html --output-dir /tmp/gh-aw/agent/cov-html
```

Pick **one** file and **one** uncovered branch. Prefer branches that are
straightforward to test (e.g., error-handling paths, edge cases, boundary
conditions). Avoid branches inside `wizard_tty.rs` or test helpers.

### 9 — Understand the uncovered branch

Read the source file and understand what scenario triggers the uncovered branch.
Identify the function, the condition, and what input would reach that branch.

### 10 — Write a test

Add a test in the appropriate test module (unit test in the same file, or
integration test under `tests/`). Follow the existing test style in the codebase.

Requirements:
- The test must compile: `cargo build --workspace`
- The test must pass: `cargo test --workspace`
- Clippy must be clean: `cargo clippy --workspace -- -D warnings`
- Formatting must pass: `cargo fmt --check`

### 11 — Verify coverage improved

Re-run coverage and confirm the branch you targeted is now covered:

```bash
cargo +nightly llvm-cov clean --workspace
cargo +nightly llvm-cov --no-report --workspace --branch
cargo +nightly llvm-cov --no-report --doc
cargo +nightly llvm-cov report --doctests --branch \
  --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs)'
```

Compare the before/after branch percentages.

### 12 — Open a Pull Request

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

### 13 — Nothing to do?

If coverage is already at 100% or all remaining uncovered branches are in
excluded files (`wizard_tty.rs`, `tests/`, etc.), call the `noop` safe output
with a message like:
> "Coverage analysis complete — no actionable uncovered branches found.
> Current branch coverage: XX.XX%."
