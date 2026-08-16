---
description: >
  Coverage improver — runs every 15 minutes. Checks open coverage-improver PRs
  for mergeability, CI status, and Copilot review comments, repairing or
  superseding unmergeable PRs instead of stalling on them. If the PR needs no
  further changes, queues the build and enables auto-merge. If no open PR
  exists, runs Rust branch coverage analysis, identifies one uncovered branch,
  writes a test to cover it, and opens a PR explaining the scenario that the
  new test covers. Repeated no-progress cycles escalate to an issue instead of
  silently reporting success.
on:
  schedule:
    - cron: "*/15 * * * *"
  workflow_dispatch:
permissions:
  contents: read
  issues: read
  pull-requests: read
  checks: read
  actions: read
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
    title-prefix: "[coverage-improver] "
    labels: [coverage-improver]
    expires: 7
    deduplicate-by-title: true
  noop:
    report-as-issue: false
---

# Coverage Improver

You are an expert Rust developer improving branch coverage for this project.
The project enforces a strict **89% branch-coverage gate** (see `CLAUDE.md`).

## Goal

On each run, first check whether an open `[coverage-improver]` PR already
exists. If it does, verify it is actually capable of making progress
(mergeable, checks running, recently active) before deciding what to do —
never park the workflow on a PR that cannot merge. If the PR is healthy,
inspect it for unresolved Copilot review comments and act accordingly. If no
PR exists, find **one** uncovered branch, write the smallest possible test
that covers it, and open a PR.

**Forward progress is mandatory.** A `noop` is only acceptable when the open
PR is mergeable, its checks are green or still running, and it has been
waiting less than 24 hours. Every other state — conflicts, failing checks,
missing checks, staleness — requires a repair, a supersede, or an escalation,
never a silent `noop`.

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

- If **an open PR is found**, go to **Step 2** (verify it can make progress).
- If **no open PR is found**, go to **Step 7** (create a new PR).

### 2 — Verify the PR can make progress

Before looking at review comments, establish whether the PR is actually
capable of merging. Gather its mergeable state, merge state status, status
check rollup, and last-updated timestamp — e.g.:

```bash
gh pr view <n> --json mergeable,mergeStateStatus,statusCheckRollup,updatedAt
```

(or the equivalent GitHub MCP pull-request and check-status reads).

Decide based on the FIRST condition that matches:

- **`mergeable: CONFLICTING` / `mergeStateStatus: DIRTY`** → go to
  **Step 3** (repair or supersede). Do NOT `noop`.
- **Any check is failing or cancelled**, and there are no unresolved review
  comments explaining the failure → go to **Step 4** (fix the failures).
  Do NOT `noop`.
- **No checks at all** on the head commit and the PR has been unchanged for
  **more than 2 hours** → the CI trigger was lost (e.g. the PR was created or
  updated with a token whose events GitHub suppresses). Treat as stuck: go to
  **Step 3** and use the supersede path. Do NOT `noop`.
- **Unchanged for more than 24 hours** (`updatedAt` older than 24h) without
  merging, regardless of state → stale: go to **Step 3** and use the
  supersede path. Do NOT `noop`.
- **Otherwise** (mergeable, checks green or pending, recently active) →
  go to **Step 5** (handle review comments).

### 3 — Repair or supersede an unmergeable PR

The existing PR cannot make progress as-is. **First try to repair it:**

1. Check out the PR branch and rebase it onto the latest `main`, resolving
   any conflicts. If the branch history is too broken to rebase cleanly,
   skip to the supersede path below.
2. Verify the repaired branch:

   ```bash
   cargo build --workspace
   cargo test --workspace
   cargo clippy --workspace -- -D warnings
   cargo fmt --check
   ```

3. Push the repair with the `push-to-pull-request-branch` safe output, then
   **stop** — CI will re-run on the next push.

**If repair is not feasible** (extensive conflicts, broken branch, or the
missing-CI-trigger case), supersede the PR instead:

1. Use the `close-pull-request` safe output to close the PR, with a comment
   explaining why, e.g.:
   > "Closing as superseded: this PR is unmergeable with `main` (or has had
   > no CI checks for over 2 hours). A fresh coverage PR will be opened from
   > current `main`."
2. Continue to **Step 7** to create a fresh PR from current `main`.

**Never** call `noop` for a PR that reached this step.

### 4 — Fix failing checks

The PR has failing or cancelled CI checks but no actionable review comments.

1. Read the failing check runs for the PR head commit and inspect the failed
   job logs to diagnose the failure.
2. Apply the fix, following all lint rules.
3. Verify locally:

   ```bash
   cargo build --workspace
   cargo test --workspace
   cargo clippy --workspace -- -D warnings
   cargo fmt --check
   ```

4. Push the fix with the `push-to-pull-request-branch` safe output, then
   **stop** — CI will re-run.

If the failures cannot be fixed (e.g. the PR's approach is fundamentally
broken), fall back to the supersede path in **Step 3**.

**Never** call `noop` for a PR with failing checks.

### 5 — Inspect for Copilot review comments

Read all open (unresolved) review threads on the existing PR. Focus on comments
left by Copilot or the `github-actions` bot that request code changes.

- If **there are unresolved review comments requesting code changes**,
  go to **Step 6** (apply the updates).
- If **there are no actionable review comments** (comments are resolved,
  informational only, or there are none at all),
  go to **Step 6b** (confirm the PR is ready).

### 6 — Apply review comment updates

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

### 6b — Confirm the PR is ready (with no-op guard)

The PR is mergeable, its checks are green or pending, and there are no
actionable review comments. Before calling `noop`, compute how long the PR
has been waiting: **now − `updatedAt`**.

- If the PR has been waiting **more than 24 hours** with no merge, no push,
  and no new review activity, the workflow is livelocked on this PR. Do NOT
  `noop`. Instead, use the `create-issue` safe output to escalate:
  - **Title**: `Coverage PR stuck — no progress in over 24h`
  - **Body**: the PR number, its mergeable state and merge state status, a
    summary of the check rollup, the `updatedAt` timestamp, and a concrete
    recommendation (close as superseded, or request maintainer review).
  Keep the title exactly as written so duplicate escalations are deduplicated
  while one is already open.
  *Rationale: every `noop` leaves the PR untouched, so N consecutive no-ops
  on the same PR are exactly equivalent to the PR going unchanged for
  N × 15 minutes. The 24-hour rule IS the consecutive-no-op guard — and it
  produces a visible issue instead of a clean-looking `success`.*
- Otherwise, call the `noop` safe output with a message that includes the
  wait duration, so run logs show the waiting progression, e.g.:
  > "No outstanding review comments on PR #N; mergeable and waiting on
  > CI/review for 3h20m. Auto-merge will trigger once checks pass."

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
