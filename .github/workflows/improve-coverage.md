---
description: >
  Coverage improver — runs every 15 minutes. Checks open coverage-improver PRs
  for Copilot review comments and applies any requested updates. Verifies an
  open PR can actually merge and repairs or supersedes conflicting, failing,
  or stalled PRs instead of no-oping on them. If the PR needs no further
  changes, queues the build and enables auto-merge. If no open PR exists, runs
  Rust branch coverage analysis, identifies one uncovered branch, writes a test
  to cover it, and opens a PR explaining the scenario that the new test covers.
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
    title-prefix: "[coverage-improver] "
    labels: [coverage-improver]
    max: 1
    # Escalation issues must not auto-expire: an auto-closed alert would make
    # a livelocked workflow indistinguishable from a healthy one again.
    expires: false
  noop:
    report-as-issue: false
---

# Coverage Improver

You are an expert Rust developer improving branch coverage for this project.
The project enforces a strict **89% branch-coverage gate** (see `CLAUDE.md`).

## Goal

On each run, first check whether an open `[coverage-improver]` PR already
exists. If it does, verify it can actually make forward progress — a PR that
is conflicting, failing checks, or stalled must be repaired or superseded,
never parked with a `noop`. Then inspect it for unresolved Copilot review
comments and act accordingly. If no PR exists, find **one** uncovered branch,
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

- If **an open PR is found**, go to **Step 2** (check mergeability).
- If **no open PR is found**, go to **Step 7** (create a new PR).

### 2 — Check the PR can actually merge

Before deciding anything, verify the existing PR is *capable* of making forward
progress. **No outstanding review comments does not mean healthy** — the PR may
be unmergeable, unverified, or stalled, and `noop`-ing on such a PR parks this
workflow indefinitely (see #1391). Check:

```bash
gh pr view <n> --json mergeable,mergeStateStatus,statusCheckRollup,updatedAt
```

Evaluate in order:

1. **`mergeable: CONFLICTING`** → go to **Step 3** (repair or supersede).
2. **Failing status checks** (any `statusCheckRollup` entry with a failing
   conclusion) **with no unresolved review comments** → diagnose the failures
   and fix them following **Step 5**. Do **not** `noop`.
3. **No checks at all** and `updatedAt` older than **24 hours** → the PR is
   stuck; go to **Step 3** (supersede it).
4. **Stale**: `updatedAt` older than **48 hours** with no new commits, merges,
   or review activity → go to **Step 3** (supersede it).
5. Otherwise the PR is healthy → go to **Step 4** (inspect review comments).

### 3 — Repair or supersede an unmergeable PR

**Repair first** when the PR is `CONFLICTING`:

1. Check out the PR branch, merge `origin/main` into it (or rebase onto it),
   and resolve the conflicts, following all lint rules.
2. Verify the resolution:

   ```bash
   cargo build --workspace
   cargo test --workspace
   cargo clippy --workspace -- -D warnings
   cargo fmt --check
   ```

3. Push with the `push-to-pull-request-branch` safe output, then **stop** —
   CI re-runs and the next scheduled run picks the PR up again.

**Supersede** when repair is not possible — conflicts that cannot be resolved
cleanly, or a PR that is stuck (no checks after 24 h) or stale (Step 2):

1. Use the `close-pull-request` safe output to close the PR with a comment
   such as:
   > "Superseded: this PR can no longer merge (conflicting/stale). A fresh
   > coverage PR will be opened automatically."
2. Continue to **Step 7** and open a fresh coverage PR in this same run.

Never `noop` on an unmergeable or stalled PR: repairing or closing it is the
only acceptable outcome.

### 4 — Inspect for Copilot review comments

Read all open (unresolved) review threads on the existing PR. Focus on comments
left by Copilot or the `github-actions` bot that request code changes.

- If **there are unresolved review comments requesting code changes**,
  go to **Step 5** (apply the updates).
- If **there are no actionable review comments** (comments are resolved,
  informational only, or there are none at all),
  go to **Step 6** (confirm the PR is ready).

### 5 — Apply review comment or CI failure updates

For each unresolved review comment that requests a code change, and for each
failing status check routed here from Step 2:

1. Read the affected source file (or the failing check's logs) and understand
   the required change.
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

### 6 — Confirm the PR is ready — and escalate repeated no-ops

There are no actionable review comments and the PR passed the Step 2
mergeability checks. Before `noop`-ing, check whether this run is part of a
**repeated no-op pattern** — a `noop`-only run still exits `success`, so a
parked workflow looks healthy on the dashboard (#1391):

1. If the PR's `updatedAt` is older than **24 hours** (earlier runs have
   almost certainly `noop`'d on this same PR already):
   - Search for an existing **open** issue labelled `coverage-improver` whose
     title references this PR number.
   - If none exists, use the `create-issue` safe output with a title like
     `[coverage-improver] PR #N appears stuck — repeated no-op cycles` and a
     body containing the PR number, its `mergeable` / `mergeStateStatus` /
     `statusCheckRollup` state, and its `updatedAt`. If an escalation issue is
     already open, do **not** create a duplicate — the signal is visible.
2. Call the `noop` safe output with a message such as:
   > "No outstanding review comments found on PR #N. Auto-merge will trigger once
   > all checks pass."

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
