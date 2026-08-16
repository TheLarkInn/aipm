---
description: >
  Coverage improver — runs every 15 minutes. Checks open coverage-improver PRs
  for Copilot review comments and applies any requested updates. Before that,
  verifies the open PR can still make progress: a conflicting, stale, or
  check-less PR is closed as superseded (with a visible escalation issue) and
  replaced by a fresh one instead of being noop'd on forever. If the PR needs
  no further changes, queues the build and enables auto-merge. If no open PR
  exists, runs Rust branch coverage analysis, identifies one uncovered branch,
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
  # Guard against livelock (#1391): a conflicting, stale, or check-less
  # coverage PR must be closed as superseded and surfaced via an escalation
  # issue instead of being noop'd on indefinitely while exiting `success`.
  close-pull-request:
    target: "*"
    required-title-prefix: "[coverage-improver]"
    max: 1
  create-issue:
    max: 1
    labels: [coverage-improver]
    # Exact-title dedup: repeated escalations about the same stuck PR do not
    # open duplicate issues.
    deduplicate-by-title: true
    # Escalation issues must not silently expire — a livelock that auto-closes
    # its own alarm is indistinguishable from success.
    expires: false
  noop:
    report-as-issue: false
---

# Coverage Improver

You are an expert Rust developer improving branch coverage for this project.
The project enforces a strict **89% branch-coverage gate** (see `CLAUDE.md`).

## Goal

On each run, first check whether an open `[coverage-improver]` PR already
exists. If it does, verify the PR is still capable of making progress — an open
PR that is `CONFLICTING`, stuck without checks, or stale is a livelock, not a
queue: close it as superseded, escalate visibly, and start fresh. Otherwise
inspect it for unresolved Copilot review comments and act accordingly. If no PR
exists (or the existing PR was just superseded), find **one** uncovered branch,
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

- If **an open PR is found**, go to **Step 2** (inspect mergeability).
- If **no open PR is found**, go to **Step 6** (create a new PR).

### 2 — Inspect the PR's mergeability and health

An open PR is not necessarily a PR that can merge. Before deciding what to do,
inspect its mergeability:

```bash
gh pr view <N> --json mergeable,mergeStateStatus,statusCheckRollup,updatedAt
```

Decide based on the result:

- **`mergeable` is `CONFLICTING`** → the PR can never merge as-is. Go to
  **Step 2a** (supersede the PR). Do **not** `noop`.
- **Any entry in `statusCheckRollup` has a failing conclusion** (`FAILURE`,
  `TIMED_OUT`, `CANCELLED`, `ACTION_REQUIRED`) **and there are no unresolved
  review comments explaining the failure** → the failing checks *are* the
  feedback: go to **Step 4** and fix the failures instead of applying review
  comments. Do **not** `noop`.
- **`statusCheckRollup` is empty and the PR's `updatedAt` is more than 2 hours
  old** → this workflow runs every 15 minutes, so checks should have started
  long ago; the PR is stuck. Go to **Step 2a**. Do **not** `noop`.
- **The PR's `updatedAt` is more than 48 hours old and there are no unresolved
  review comments** → the PR is stale. A PR untouched that long has already
  been `noop`'d by ~190 consecutive runs — treat that as the repeated-no-op
  escalation trigger. Go to **Step 2a**. Do **not** `noop`.
- **Otherwise** (`MERGEABLE`, checks green or still legitimately running, and
  recently active) → go to **Step 3** (inspect review comments).

#### 2a — Supersede a stuck or unmergeable PR

A stuck PR must never park the workflow, and closing it must not be silent:

1. Use the `close-pull-request` safe output to close the PR, with a comment
   stating which condition triggered the supersede (conflicting / failing
   checks with no review feedback / no checks 2 hours after creation / stale
   beyond 48 hours) and that a fresh PR will replace it.
2. Use the `create-issue` safe output to open an escalation issue titled
   `[coverage-improver] Superseded stuck PR #N (<reason>)` whose body records
   the PR's `mergeable`, `mergeStateStatus`, check conclusions, and
   `updatedAt`. This is the livelock alarm: a supersede is visible to humans
   instead of reporting clean `success` via `noop`. Titles are deduplicated,
   so a PR that is somehow re-encountered does not open duplicate issues.
3. Continue to **Step 6** (run coverage analysis and open a fresh PR against
   current `main`). Coverage tests are cheap to regenerate; a fresh PR is
   guaranteed conflict-free, so do not attempt to rebase the closed branch.

### 3 — Inspect for Copilot review comments

Read all open (unresolved) review threads on the existing PR. Focus on comments
left by Copilot or the `github-actions` bot that request code changes.

- If **there are unresolved review comments requesting code changes**,
  go to **Step 4** (apply the updates).
- If **there are no actionable review comments** (comments are resolved,
  informational only, or there are none at all),
  go to **Step 5** (queue the build and enable auto-merge).

### 4 — Apply review comment updates / fix failing checks

For each unresolved review comment that requests a code change:

1. Read the affected source file and understand the requested change.
2. Apply the change, following all lint rules.

If you arrived here from **Step 2** because checks are failing without any
review comments, read the failing checks instead (`gh pr checks <N>`, then
`gh run view --log <run-id>` for the failed run), diagnose the failure, and
fix the underlying problem, following all lint rules.

In both cases, before pushing:

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

This step is only reachable for a **healthy** PR: `MERGEABLE`, checks green or
actively running, and `updatedAt` within the last 48 hours (Step 2 routes every
other state elsewhere). There is genuinely nothing to do but wait for CI and
auto-merge.

1. Call the `noop` safe output with a message such as:
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
