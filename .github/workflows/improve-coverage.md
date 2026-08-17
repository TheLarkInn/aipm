---
description: >
  Coverage improver — runs every 15 minutes. Checks open coverage-improver PRs
  for Copilot review comments and applies any requested updates. Before waiting
  on an existing PR it verifies the PR can actually merge — repairing conflicts,
  fixing failing checks, or closing stuck/stale PRs as superseded — and repeated
  no-op runs on the same PR escalate to an issue instead of reporting silent
  success. If no open PR exists, runs Rust branch coverage analysis, identifies
  one uncovered branch, writes a test to cover it, and opens a PR explaining
  the scenario that the new test covers.
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
  add-comment:
    target: "*"
    max: 2
  create-issue:
    title-prefix: "[coverage-improver]"
    labels: [coverage-improver]
    max: 1
  noop:
    report-as-issue: false
---

# Coverage Improver

You are an expert Rust developer improving branch coverage for this project.
The project enforces a strict **89% branch-coverage gate** (see `CLAUDE.md`).

## Goal

On each run, first check whether an open `[coverage-improver]` PR already
exists. If it does, **verify the PR can actually make progress** before doing
anything else: a conflicting, failing, stuck, or stale PR must be repaired or
superseded — never waited on. If the existing PR is healthy, inspect it for
unresolved Copilot review comments and act accordingly. If no PR exists, find
**one** uncovered branch, write the smallest possible test that covers it, and
open a PR.

A run that only `noop`s is classified as success, which is how a past livelock
(a conflicting PR parked the workflow for weeks while every run exited green)
stayed invisible. Step 8 therefore escalates repeated identical `noop`s on the
same PR to a visible issue instead of letting silence pass for success.

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

Search for open pull requests whose title contains `[coverage-improver]`. If
several are open, operate on the most recently updated one.

- If **an open PR is found**, go to **Step 2** (check its mergeability).
- If **no open PR is found**, go to **Step 9** (create a new PR).

### 2 — Inspect the PR's mergeability (required before any `noop`)

Never decide to `noop` without first checking whether the existing PR can
actually merge. Fetch its current state:

```bash
gh pr view <number> --json mergeable,mergeStateStatus,statusCheckRollup,updatedAt,headRefName,commits
```

Decide as follows:

- **`mergeable` is `CONFLICTING`** → go to **Step 3** (repair or supersede).
  Do **not** `noop`.
- **`mergeStateStatus` is `BEHIND`** (mergeable, but behind `main`) → go to
  **Step 3** (merge `main` into the PR branch and push). Do **not** `noop`.
- **Any entry in `statusCheckRollup` has concluded `FAILURE`, `TIMED_OUT`,
  `CANCELLED`, or `ACTION_REQUIRED`**:
  - If unresolved review comments already describe the failures, go to
    **Step 6** (handle review comments) — applying them usually fixes CI.
  - Otherwise go to **Step 4** (fix failing checks). Do **not** `noop`.
- **`statusCheckRollup` is empty AND `updatedAt` is more than 2 hours ago** →
  CI never started on this PR; it is stuck. Go to **Step 5** (close as
  superseded and start fresh). Do **not** `noop`.
- **The PR's most recent commit is more than 24 hours old and it is still
  unmerged** (checks failing, absent, or long green without a merge) → stale.
  Go to **Step 5** (close as superseded and start fresh). Do **not** `noop`.
- **Otherwise** (mergeable; checks green or still running normally;
  `mergeable: UNKNOWN` while GitHub computes mergeability) → the PR is
  healthy. Go to **Step 6** (handle review comments).

### 3 — Repair a conflicting or behind PR

1. Check out the PR branch locally (the checkout fetches every branch) and
   merge `main` into it:

   ```bash
   git checkout <headRefName>
   git merge origin/main
   ```

   Prefer merging over rebasing — a merge is a plain fast-forwardable push,
   while a rebase would require a force-push.

2. Resolve any conflicts, keeping the PR's intent. If the branch the PR set
   out to cover is already covered on `main`, or the conflicts cannot be
   resolved cleanly, do not force it: close the PR as described in **Step 5**
   and continue to **Step 9** (fresh coverage run) instead.

3. Verify the repaired branch:

   ```bash
   cargo build --workspace
   cargo test --workspace
   cargo clippy --workspace -- -D warnings
   cargo fmt --check
   ```

4. Use the `push-to-pull-request-branch` safe output to push the merge commit
   to the existing PR branch.

After pushing, **stop** — CI will re-run on the updated branch.

### 4 — Fix failing checks

1. Identify what failed:

   ```bash
   gh pr checks <number>
   gh run view --log-failed <run-id>
   ```

2. Fix the failures, following all lint rules.
3. Verify the code compiles, tests pass, clippy is clean, formatting passes,
   and coverage still meets the 89% branch gate (commands as in Step 3 and
   Step 7).
4. Use the `push-to-pull-request-branch` safe output to push the fix to the
   existing PR branch.

After pushing, **stop** — CI will re-run on the updated branch.

### 5 — Close a stuck or stale PR as superseded

1. Use the `close-pull-request` safe output to close the PR, with a comment
   explaining why, e.g.:
   > "Superseded: this PR has merge conflicts / failing checks / no CI after
   > 2 hours / no progress for 24 hours. A fresh coverage PR replaces it."
2. **Continue to Step 9** — run a fresh coverage analysis and open a
   replacement PR in this same run. Do not stop and do not `noop`: the run
   must make forward progress.

### 6 — Inspect for Copilot review comments

Read all open (unresolved) review threads on the existing PR. Focus on comments
left by Copilot or the `github-actions` bot that request code changes.

- If **there are unresolved review comments requesting code changes**,
  go to **Step 7** (apply the updates).
- If **there are no actionable review comments** (comments are resolved,
  informational only, or there are none at all),
  go to **Step 8** (confirm the PR is ready — with the noop escalation guard).

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

### 8 — Confirm the PR is ready (noop escalation guard)

The PR passed the Step 2 mergeability check and has no actionable review
comments, so the correct outcome is usually to wait for checks and auto-merge.
But an unbroken streak of identical `noop`s on the same PR must surface
visibly instead of reporting clean success forever — that silence is exactly
how the previous livelock hid. Apply this guard:

1. **Count the waiting streak.** List the PR's comments
   (`gh pr view <number> --json comments`, or the GitHub issue-comments tool
   for the PR number). Scanning from newest to oldest, count the trailing
   comments whose body contains the marker `<!-- coverage-improver-noop -->`.
   Stop at the first comment without the marker — any newer commit, review,
   or human comment resets the streak. Call the count **C**.

2. **C < 8** — keep waiting, but leave a visible trace. Post a marker comment
   via the `add-comment` safe output:

   > `<!-- coverage-improver-noop -->` ⏳ Coverage Improver: PR is mergeable
   > with no outstanding review comments; waiting on checks/auto-merge.
   > (Consecutive waiting runs: C+1/8 — the 8th escalates instead of waiting.)

   Then call the `noop` safe output with a message such as:
   > "No outstanding review comments found on PR #N (waiting streak C+1/8).
   > Auto-merge will trigger once all checks pass."

   **Stop** — do not run coverage analysis or create a new PR.

3. **C ≥ 8** (about 2 hours with zero progress) — escalate; do **not** `noop`.
   Re-run the Step 2 mergeability check once more and act on the result:

   - Now `CONFLICTING` or `BEHIND` → go to **Step 3**.
   - Failing checks → go to **Step 4**.
   - Still no checks after 2+ hours → go to **Step 5** (supersede and rebuild).
   - Checks green, `mergeable: MERGEABLE`, auto-merge enabled, yet still
     unmerged → something outside this workflow's control is blocking the
     merge (branch protection, a required review, a stuck merge queue).
     Escalate to a human:
     - Search for an existing open escalation issue first
       (`gh issue list --search 'in:title "[coverage-improver] PR #N"' --state open`).
       If one already exists, do not open a duplicate and do not add another
       comment — call `noop` and stop. The open issue is the visible signal;
       repeated comments would be noise.
     - Otherwise use the `create-issue` safe output with title
       `PR #N is healthy but not merging — auto-merge appears stuck` and a
       body containing the Step 2 diagnostics (`mergeable`,
       `mergeStateStatus`, a check summary, `updatedAt`, the waiting streak
       length) and a request for a maintainer to merge or unblock it manually.

     While the escalation issue is open, later runs `noop` quietly at this
     point — the open issue, not the run log, is now the visible signal.

### 9 — Collect branch-level coverage

No open PR exists (either none was found, or a stuck/stale one was superseded
in Step 5). Run a fresh coverage analysis:

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
