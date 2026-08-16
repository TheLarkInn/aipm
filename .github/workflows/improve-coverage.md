---
description: >
  Coverage improver — runs every 15 minutes. Checks open coverage-improver PRs
  for Copilot review comments and applies any requested updates. Before waiting
  on an existing PR it verifies the PR can still merge: conflicting, failing,
  checkless, or stale PRs are repaired or closed and superseded so the workflow
  cannot livelock, and repeated no-op cycles escalate to an issue. If the PR
  needs no further changes, queues the build and enables auto-merge. If no open
  PR exists, runs Rust branch coverage analysis, identifies one uncovered branch,
  writes a test to cover it, and opens a PR explaining the scenario that the
  new test covers.
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
  # Persists the consecutive-noop counter across runs (see Step 2 and Step 6).
  cache-memory: true
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
  # Closes superseded coverage PRs so an unmergeable PR cannot block new work.
  close-pull-request:
    target: "*"
    required-title-prefix: "[coverage-improver]"
    max: 1
  # Escalates repeated no-op cycles — a livelocked workflow must not look
  # identical to a healthy one.
  create-issue:
    max: 1
    title-prefix: "[coverage-improver] "
    labels: [agentic-workflows]
    deduplicate-by-title: true
  noop:
    report-as-issue: false
---

# Coverage Improver

You are an expert Rust developer improving branch coverage for this project.
The project enforces a strict **89% branch-coverage gate** (see `CLAUDE.md`).

## Goal

On each run, first check whether an open `[coverage-improver]` PR already
exists. If it does, verify the PR is still capable of merging — a PR that is
`CONFLICTING`, has failing or absent checks, or has gone stale must be repaired
or superseded, never waited on. Waiting on an unmergeable PR is how this
workflow livelocked on PR #887 while still reporting `success`. Then inspect
it for unresolved Copilot review comments and act accordingly. If no PR
exists, find **one** uncovered branch, write the smallest possible test that
covers it, and open a PR.

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
- If **no open PR is found**, go to **Step 7** (create a new PR).

### 2 — Assess PR health and liveness

Before looking at review comments, determine whether the open PR is actually
capable of merging. Collect its state:

```bash
gh pr view <N> --json number,title,headRefName,mergeable,mergeStateStatus,statusCheckRollup,createdAt,updatedAt
```

Also read the no-op cycle counter from cache memory at
`/tmp/gh-aw/cache-memory/coverage-improver-noop.json`:

```json
{"pr_number": 123, "consecutive_noops": 4, "escalated": false}
```

If the file does not exist, or `pr_number` does not match the current PR,
treat the counter as zero and `escalated` as false.

Decision table — apply the **first** matching row:

| Condition | Action |
|---|---|
| `mergeable` is `CONFLICTING` (or `mergeStateStatus` is `DIRTY`) | Go to **Step 3** (repair or supersede). Do **not** `noop`. |
| Any `statusCheckRollup` entry has conclusion `FAILURE`, `TIMED_OUT`, `CANCELLED`, or `ACTION_REQUIRED`, and there are no actionable review comments | Go to **Step 5** (fix the failures and push). Do **not** `noop`. |
| `statusCheckRollup` is empty **and** `createdAt` is more than **2 hours** ago | Stuck with no checks — go to **Step 3** (supersede). Do **not** `noop`. |
| `updatedAt` is more than **48 hours** ago | Stale — go to **Step 3** (supersede). Do **not** `noop`. |
| Otherwise (checks pending or passing, PR recently active) | Go to **Step 4** (inspect review comments). |

### 3 — Repair or supersede an unmergeable PR

**Preferred: repair.** Merge `origin/main` into the PR branch, resolve any
conflicts, verify the build, and push via the `push-to-pull-request-branch`
safe output:

```bash
git fetch origin main
git checkout <headRefName>
git merge origin/main
# resolve conflicts, keeping the PR's intent intact
cargo build --workspace
cargo test --workspace
```

Use a merge commit rather than a rebase so the safe output can push
fast-forward. After pushing, reset the no-op counter (delete
`/tmp/gh-aw/cache-memory/coverage-improver-noop.json`) and **stop** — CI will
re-run on the updated branch.

**If the conflicts are non-trivial or the merge cannot be made green within
this run**, supersede the PR instead:

1. Call the `close-pull-request` safe output for the PR with a comment
   explaining that it is unmergeable or stale and is being superseded by a
   fresh coverage PR.
2. Reset the no-op counter (delete the counter file).
3. Continue to **Step 7** to create a replacement PR in this same run.

### 4 — Inspect for Copilot review comments

Read all open (unresolved) review threads on the existing PR. Focus on comments
left by Copilot or the `github-actions` bot that request code changes.

- If **there are unresolved review comments requesting code changes**,
  go to **Step 5** (apply the updates).
- If **there are no actionable review comments** (comments are resolved,
  informational only, or there are none at all),
  go to **Step 6** (confirm the PR is ready).

### 5 — Apply review comment or check failure updates

Apply each change requested by unresolved review comments, or — when Step 2
routed you here because checks are failing — diagnose and fix the failing
checks:

1. Read the affected source file and understand the requested change or
   failure.
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
6. Reset the no-op counter (delete
   `/tmp/gh-aw/cache-memory/coverage-improver-noop.json`).

After pushing, **stop** — the CI pipeline will re-run and Copilot will
re-review if needed. The next scheduled run will pick up any new comments.

### 6 — Confirm the PR is ready

If the PR is healthy (Step 2 found no livelock symptoms) and there are no
actionable review comments:

1. Increment the no-op counter — write
   `/tmp/gh-aw/cache-memory/coverage-improver-noop.json` with
   `{"pr_number": <N>, "consecutive_noops": <previous + 1>, "escalated": <previous>}`.
2. If the counter reaches **96** (≈24 hours at the 15-minute cadence) and
   `escalated` is still false, call the `create-issue` safe output with a
   title such as `PR #N has not progressed for 24h` (the configured
   title-prefix is applied automatically) and a body describing the PR's
   `mergeable` / `mergeStateStatus` / check state, then mark `escalated: true`
   in the counter file. A healthy-looking PR that never merges is a stuck PR —
   repeated no-op cycles must surface visibly, not report clean success.
3. Call the `noop` safe output with a message such as:
   > "No outstanding review comments found on PR #N (no-op cycle X of 96).
   > Auto-merge will trigger once all checks pass."

**Stop** — do not run coverage analysis or create a new PR.

### 7 — Collect branch-level coverage

No open PR exists. Run a fresh coverage analysis:

```bash
cargo +nightly llvm-cov clean --workspace
cargo +nightly llvm-cov --no-report --workspace --branch
cargo +nightly llvm-cov --no-report --doc
```

### 8 — Generate a detailed per-file report

```bash
cargo +nightly llvm-cov report --doctests --branch \
  --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs)'
```

Save the full output. Note the overall branch percentage.

### 9 — Find uncovered branches

Run the HTML or text report to locate files with uncovered branches:

```bash
cargo +nightly llvm-cov report --doctests --branch \
  --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs)' \
  --html --output-dir /tmp/gh-aw/agent/cov-html
```

Pick **one** file and **one** uncovered branch. Prefer branches that are
straightforward to test (e.g., error-handling paths, edge cases, boundary
conditions). Avoid branches inside `wizard_tty.rs` or test helpers.

### 10 — Understand the uncovered branch

Read the source file and understand what scenario triggers the uncovered branch.
Identify the function, the condition, and what input would reach that branch.

### 11 — Write a test

Add a test in the appropriate test module (unit test in the same file, or
integration test under `tests/`). Follow the existing test style in the codebase.

Requirements:
- The test must compile: `cargo build --workspace`
- The test must pass: `cargo test --workspace`
- Clippy must be clean: `cargo clippy --workspace -- -D warnings`
- Formatting must pass: `cargo fmt --check`

### 12 — Verify coverage improved

Re-run coverage and confirm the branch you targeted is now covered:

```bash
cargo +nightly llvm-cov clean --workspace
cargo +nightly llvm-cov --no-report --workspace --branch
cargo +nightly llvm-cov --no-report --doc
cargo +nightly llvm-cov report --doctests --branch \
  --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs)'
```

Compare the before/after branch percentages.

### 13 — Open a Pull Request

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

### 14 — Nothing to do?

If coverage is already at 100% or all remaining uncovered branches are in
excluded files (`wizard_tty.rs`, `tests/`, etc.), call the `noop` safe output
with a message like:
> "Coverage analysis complete — no actionable uncovered branches found.
> Current branch coverage: XX.XX%."
