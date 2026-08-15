---
description: >
  Coverage improver — runs every 15 minutes. Checks open coverage-improver PRs
  for Copilot review comments and applies any requested updates. If the PR needs
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
  # Needed so the agent can list recent runs and read their safe-output logs
  # when detecting repeated no-op livelocks (Step 4).
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
  # Lets the workflow retire a coverage PR that can never merge (conflicting,
  # or stuck with no checks) instead of no-op looping on it forever. Scoped by
  # title prefix so it can only ever close this workflow's own PRs.
  close-pull-request:
    target: "*"
    required-title-prefix: "[coverage-improver]"
    max: 1
  # Escalation channel for the repeated-no-op livelock guard (Step 4): when the
  # workflow detects it has no-op'd on the same PR repeatedly, it files an
  # issue instead of staying silently "green".
  create-issue:
    max: 1
    labels: [coverage-improver]
  noop:
    report-as-issue: false
---

# Coverage Improver

You are an expert Rust developer improving branch coverage for this project.
The project enforces a strict **89% branch-coverage gate** (see `CLAUDE.md`).

## Goal

On each run, first check whether an open `[coverage-improver]` PR already
exists. If it does, inspect it for unresolved Copilot review comments and act
accordingly. If no PR exists, find **one** uncovered branch, write the smallest
possible test that covers it, and open a PR.

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

- If **an open PR is found**, go to **Step 1a** (check it can still merge).
- If **no open PR is found**, go to **Step 5** (create a new PR).

### 1a — Check the existing PR is still mergeable

An open PR only justifies pausing new coverage work if it is actually
capable of merging. Inspect it before doing anything else:

```bash
gh pr view <N> --json mergeable,mergeStateStatus,statusCheckRollup,updatedAt,url
```

Decide as follows, in order:

1. **`mergeable` is `CONFLICTING`** (or `mergeStateStatus` is `DIRTY`) — the PR
   can never auto-merge. Rebase the branch onto the current `main` and resolve
   the conflicts, re-running the build, tests and clippy afterwards, then push
   with `push-to-pull-request-branch`. If the branch it covers has since been
   covered by another merged PR, or the conflicts are not worth resolving,
   close it with the `close-pull-request` safe output and continue to
   **Step 5** to open a fresh one. Do **not** call `noop`.

2. **Checks are failing** (`statusCheckRollup` contains a `FAILURE`) and there
   are no review comments explaining them — fix the failures and push. Do
   **not** call `noop`.

3. **There are no checks at all** and `updatedAt` is more than 24 hours ago —
   treat the PR as stuck. Close it with the `close-pull-request` safe output
   and continue to **Step 5**. Do **not** call `noop`.

4. **`updatedAt` is more than 48 hours ago** with no progress — the PR is
   stale; a healthy auto-merge PR lands well within this window. Close it as
   superseded with the `close-pull-request` safe output and continue to
   **Step 5** to start fresh.

5. **Otherwise** the PR is healthy — go to **Step 2** (handle review comments).

> **Why this step exists.** PR #887 sat `CONFLICTING` with auto-merge enabled
> and zero checks from 2026-05-12 onward. Every scheduled run found it, saw no
> review comments, emitted "Auto-merge will trigger once all checks pass", and
> stopped — for weeks. Auto-merge could never trigger. Because a run that emits
> only `noop` is classified as success, nothing ever alerted. Never `noop` on a
> PR that cannot merge.

### 2 — Inspect for Copilot review comments

Read all open (unresolved) review threads on the existing PR. Focus on comments
left by Copilot or the `github-actions` bot that request code changes.

- If **there are unresolved review comments requesting code changes**,
  go to **Step 3** (apply the updates).
- If **there are no actionable review comments** (comments are resolved,
  informational only, or there are none at all),
  go to **Step 4** (queue the build and enable auto-merge).

### 3 — Apply review comment updates

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

### 4 — Confirm the PR is ready (with repeated-no-op livelock guard)

Only reach this step for a PR that passed the mergeability checks in **Step 1a**
— that is, one that is not conflicting, has checks, and is not stale.

A run that produces only `noop` is classified as **success**, so consecutive
no-ops are invisible on the Actions dashboard. Before calling `noop`, check
whether recent runs already no-op'd on this same PR:

1. List the most recent completed runs of this workflow:

   ```bash
   gh run list --workflow improve-coverage.lock.yml --limit 8 \
     --json databaseId,conclusion,createdAt
   ```

2. For each recent successful run, inspect the `Process Safe Outputs` job log
   for a `noop` entry referencing this PR number:

   ```bash
   gh run view <run-id> --log | grep -i "noop" | grep "#<N>"
   ```

3. If the same PR has been no-op'd **3 or more consecutive times** without its
   state changing (`updatedAt` unchanged), do **not** call `noop` again.
   Instead:
   - Re-apply the **Step 1a** rules — a PR no-op'd this many times is in
     practice stale or unmergeable, so close it and continue to **Step 5**; and
   - Emit the `create-issue` safe output titled
     `[coverage-improver] Livelock suspected on PR #<N>` describing the stuck
     state (how many consecutive no-ops, the PR's mergeability and check
     status), so a human is alerted. Silence must not be indistinguishable
     from success.

4. Otherwise (fewer than 3 consecutive no-ops), re-confirm the PR is still
   mergeable and its checks are pending or passing — not failing — then call
   the `noop` safe output with a message such as:
   > "No outstanding review comments found on PR #N (mergeable, checks pending).
   > Auto-merge will trigger once all checks pass."

   If the PR is *not* mergeable, do not `noop` — return to **Step 1a** and take
   the corrective action described there.

**Stop** — do not run coverage analysis or create a new PR.

### 5 — Collect branch-level coverage

No open PR exists. Run a fresh coverage analysis:

```bash
cargo +nightly llvm-cov clean --workspace
cargo +nightly llvm-cov --no-report --workspace --branch
cargo +nightly llvm-cov --no-report --doc
```

### 6 — Generate a detailed per-file report

```bash
cargo +nightly llvm-cov report --doctests --branch \
  --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs)'
```

Save the full output. Note the overall branch percentage.

### 7 — Find uncovered branches

Run the HTML or text report to locate files with uncovered branches:

```bash
cargo +nightly llvm-cov report --doctests --branch \
  --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs)' \
  --html --output-dir /tmp/gh-aw/agent/cov-html
```

Pick **one** file and **one** uncovered branch. Prefer branches that are
straightforward to test (e.g., error-handling paths, edge cases, boundary
conditions). Avoid branches inside `wizard_tty.rs` or test helpers.

### 8 — Understand the uncovered branch

Read the source file and understand what scenario triggers the uncovered branch.
Identify the function, the condition, and what input would reach that branch.

### 9 — Write a test

Add a test in the appropriate test module (unit test in the same file, or
integration test under `tests/`). Follow the existing test style in the codebase.

Requirements:
- The test must compile: `cargo build --workspace`
- The test must pass: `cargo test --workspace`
- Clippy must be clean: `cargo clippy --workspace -- -D warnings`
- Formatting must pass: `cargo fmt --check`

### 10 — Verify coverage improved

Re-run coverage and confirm the branch you targeted is now covered:

```bash
cargo +nightly llvm-cov clean --workspace
cargo +nightly llvm-cov --no-report --workspace --branch
cargo +nightly llvm-cov --no-report --doc
cargo +nightly llvm-cov report --doctests --branch \
  --ignore-filename-regex '(tests/|research/|specs/|wizard_tty\.rs)'
```

Compare the before/after branch percentages.

### 11 — Open a Pull Request

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

### 12 — Nothing to do?

If coverage is already at 100% or all remaining uncovered branches are in
excluded files (`wizard_tty.rs`, `tests/`, etc.), call the `noop` safe output
with a message like:
> "Coverage analysis complete — no actionable uncovered branches found.
> Current branch coverage: XX.XX%."
