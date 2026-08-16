---
description: >
  Coverage improver — runs every 15 minutes. Checks open coverage-improver PRs
  for mergeability and Copilot review comments, repairing or superseding PRs
  that can no longer merge, and applies any requested updates. If the PR needs
  no further changes, queues the build and enables auto-merge. If no open PR
  exists, runs Rust branch coverage analysis, identifies one uncovered branch,
  writes a test to cover it, and opens a PR explaining the scenario that the
  new test covers. Escalates via an issue when a coverage PR is stuck instead
  of silently reporting success.
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
    title-prefix: "[coverage-improver] "
    labels: [coverage-improver]
    # Escalations recur while the underlying condition persists; keep only the
    # latest one open instead of accumulating duplicates.
    close-older-issues: true
    deduplicate-by-title: true
  noop:
    report-as-issue: false
---

# Coverage Improver

You are an expert Rust developer improving branch coverage for this project.
The project enforces a strict **89% branch-coverage gate** (see `CLAUDE.md`).

## Goal

On each run, first check whether an open `[coverage-improver]` PR already
exists. If it does, verify it can still make progress towards merging (an
unmergeable or stalled PR must never be silently tolerated), then inspect it
for unresolved Copilot review comments and act accordingly. If no PR exists,
find **one** uncovered branch, write the smallest possible test that covers
it, and open a PR.

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

- If **an open PR is found**, go to **Step 2** (verify the PR can merge).
- If **no open PR is found**, go to **Step 6** (create a new PR).

### 2 — Verify the open PR can actually merge

A `noop` on a PR that can never merge livelocks this workflow: the run reports
`success`, no alert fires, and coverage work stalls indefinitely. Before
looking at review comments, inspect the PR's mergeability:

```bash
gh pr view <number> --json mergeable,mergeStateStatus,statusCheckRollup,updatedAt,isDraft
```

Evaluate the following rules **in order** and follow the first one that
matches. Only the last rule allows a `noop`; every other rule requires
forward progress.

1. **`mergeable: CONFLICTING` (or `mergeStateStatus: DIRTY`)** — the PR cannot
   merge. Try to repair it: check out the PR branch, rebase it onto the latest
   `main`, resolve any conflicts, verify build/tests/clippy/fmt all pass, and
   push with the `push-to-pull-request-branch` safe output. If the conflicts
   cannot be resolved cleanly — for example the branch it covers is already
   covered on `main` — close the PR with the `close-pull-request` safe output
   (comment that it is superseded and why), then go to **Step 6** to open a
   fresh PR. **Do not `noop`.**
2. **Failing checks and no actionable review comments** — the PR needs code
   changes, not patience. Investigate the failing checks, fix the failures
   following all lint rules, verify locally, and push with
   `push-to-pull-request-branch`. **Do not `noop`.**
3. **No checks at all, and `updatedAt` is more than 3 hours ago** — CI never
   started and the PR is stuck. Close it with `close-pull-request` (comment
   that it is superseded because checks never ran), then go to **Step 6**.
   **Do not `noop` indefinitely.** (A fresh PR normally shows checks within
   minutes; 3 hours of silence means they never will.)
4. **Stale: `updatedAt` is more than 24 hours old and the PR is still not
   merged** — no progress is happening regardless of what the fields claim.
   Close it with `close-pull-request` (comment that it is superseded due to
   inactivity), then go to **Step 6**. Do not keep waiting on it.
5. **All checks green, `mergeable: MERGEABLE`, but the PR is still open** —
   auto-merge should have fired and did not; this is a broken-merge condition.
   Escalate with the `create-issue` safe output (title like
   `[coverage-improver] PR #N is mergeable with green checks but auto-merge
   never fired`, body describing the PR state), then **stop**. Do not `noop`.
6. **Otherwise** (checks pending/in progress, PR is fresh and mergeable) —
   the PR is healthy and merely waiting on CI. Go to **Step 3**.

### 3 — Inspect for Copilot review comments

Read all open (unresolved) review threads on the existing PR. Focus on comments
left by Copilot or the `github-actions` bot that request code changes.

- If **there are unresolved review comments requesting code changes**,
  go to **Step 4** (apply the updates).
- If **there are no actionable review comments** (comments are resolved,
  informational only, or there are none at all),
  go to **Step 5** (queue the build and enable auto-merge).

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

This step is reached only when Step 2 confirmed the PR is mergeable with
checks pending or in progress, and there are no actionable review comments:

1. Call the `noop` safe output with a message such as:
   > "No outstanding review comments found on PR #N. Auto-merge will trigger once
   > all checks pass."

**Stop** — do not run coverage analysis or create a new PR.

A `noop` is only legitimate here because Step 2 already verified the PR is
mergeable, has checks actively running, and is fresh. If any of those stop
being true on a later run, Step 2 routes to a repair, a supersede-and-reopen,
or a `create-issue` escalation instead — a stuck PR must never produce
repeated silent `noop` runs that report `success`.

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
