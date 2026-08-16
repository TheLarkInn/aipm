---
description: >
  Coverage improver — runs every 15 minutes. Checks open coverage-improver PRs
  for Copilot review comments and applies any requested updates. Guards against
  livelock: a conflicting, check-failing, or stale PR is repaired or superseded
  instead of being noop'd on forever, and repeated no-op cycles on the same PR
  escalate to an issue. If the PR needs no further changes, queues the build
  and enables auto-merge. If no open PR exists, runs Rust branch coverage
  analysis, identifies one uncovered branch, writes a test to cover it, and
  opens a PR explaining the scenario that the new test covers.
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
  close-pull-request:
    target: "*"
    required-title-prefix: "[coverage-improver]"
    max: 1
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
exists. If it does, verify the PR can actually make forward progress
(mergeable, checks healthy, not stale) **before** inspecting it for unresolved
Copilot review comments — an unmergeable PR must be repaired or superseded,
never ignored. If no PR exists, find **one** uncovered branch, write the
smallest possible test that covers it, and open a PR.

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

- If **an open PR is found**, go to **Step 2** (assess mergeability).
- If **no open PR is found**, go to **Step 9** (collect coverage and create a
  new PR).

### 2 — Assess the PR's mergeability and health

Before looking at review comments, determine whether the PR is *capable* of
merging. A PR that is conflicting, check-failing, or stale parks this workflow
forever if you only look for review comments — this exact failure livelocked
the workflow once already.

Run:

```bash
gh pr view <number> --json mergeable,mergeStateStatus,statusCheckRollup,updatedAt,headRefName
```

Evaluate the following conditions **in order** — the first match wins, and
**none of these paths may end in `noop`**:

| Condition | Action |
|---|---|
| `mergeable` is `CONFLICTING` | Go to **Step 5** (resolve conflicts or supersede) |
| One or more checks are failing **and** there are no unresolved review comments explaining them | Go to **Step 6** (fix failing checks) |
| No checks have been reported at all **and** `updatedAt` is older than **2 hours** | Go to **Step 7** (close and supersede as stuck) |
| `updatedAt` is older than **24 hours** with no forward progress (no pushes, no new comments, checks not converging) | Go to **Step 7** (close and supersede as stale) |
| None of the above — the PR is healthy and simply waiting on checks or review | Go to **Step 3** (inspect review comments) |

`mergeable` may briefly report `UNKNOWN` while GitHub computes it. Treat
`UNKNOWN` as "none of the above" on a young PR, but as a stuck signal if
`updatedAt` is older than 2 hours.

### 3 — Inspect for Copilot review comments

Read all open (unresolved) review threads on the existing PR. Focus on comments
left by Copilot or the `github-actions` bot that request code changes.

- If **there are unresolved review comments requesting code changes**,
  go to **Step 4** (apply the updates).
- If **there are no actionable review comments** (comments are resolved,
  informational only, or there are none at all),
  go to **Step 8** (confirm the PR is ready).

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
Before stopping, reset the no-op tracker (see Step 8) — a push is forward
progress.

### 5 — Resolve merge conflicts on the open PR

The PR is `CONFLICTING`. You must make forward progress — do **not** `noop`.

1. Check out the PR branch (`headRefName` from Step 2), fetch the latest
   `main`, and rebase (or merge `main` into the branch) resolving every
   conflict in favour of keeping the PR's new test intact.
2. Verify the resolved branch:

   ```bash
   cargo build --workspace
   cargo test --workspace
   cargo clippy --workspace -- -D warnings
   cargo fmt --check
   ```

3. Push the resolved branch with the `push-to-pull-request-branch` safe output.
4. Reset the no-op tracker (see Step 8) — a push is forward progress.

If the conflicts **cannot** be resolved cleanly (e.g. the covered branch no
longer exists on `main`, or the test's subject was deleted), fall through to
**Step 7** and supersede the PR instead.

### 6 — Fix failing checks on the open PR

Checks are failing and no unresolved review comment explains why. You must make
forward progress — do **not** `noop`.

1. Read the failing check logs (`gh pr checks <number>` and the run logs) and
   diagnose the failure.
2. Apply the minimal fix, following all lint rules, and verify:

   ```bash
   cargo build --workspace
   cargo test --workspace
   cargo clippy --workspace -- -D warnings
   cargo fmt --check
   ```

3. Push the fix with the `push-to-pull-request-branch` safe output.
4. Reset the no-op tracker (see Step 8) — a push is forward progress.

If the failure is **not fixable** from the PR branch (e.g. the PR's approach is
fundamentally broken), fall through to **Step 7** and supersede the PR instead.

### 7 — Close and supersede a stuck PR

The open PR cannot make forward progress: it is stuck with no checks, stale
beyond 24 hours, or unfixable from Steps 5/6. Close it so a fresh PR can take
its place.

1. Call the `close-pull-request` safe output targeting the PR, with a comment
   explaining **why** it is being closed (conflicting / failing checks / no
   checks after 2h / stale beyond 24h / unfixable) and stating that it is
   superseded by a fresh PR to follow.
2. Reset the no-op tracker (see Step 8) — closing is forward progress.
3. Continue to the coverage analysis path — i.e. proceed exactly as if
   no open PR had been found, starting at **Step 9** (collect coverage), so a
   fresh PR is opened this run.

### 8 — Confirm the PR is ready (tracked no-op)

The PR is healthy and there are no actionable review comments. This is the
**only** path that may end in `noop`, and it is tracked so that repeated
no-op cycles on the same PR become visible instead of reporting clean success
forever.

1. Read the no-op tracker file at `/tmp/gh-aw/cache-memory/noop-tracker.json`
   (cache-memory persists across runs). If it does not exist, treat it as
   `{"pr": 0, "count": 0, "escalated": false}`.
2. If the tracker's `pr` equals the current PR number, increment `count` by 1.
   Otherwise write a fresh tracker: `{"pr": <number>, "count": 1, "escalated": false}`.
3. If `count` reaches **20** (≈ 5 hours of consecutive no-op runs at the
   15-minute cadence) **and** `escalated` is still `false`:
   - Call the `create-issue` safe output with title
     `[coverage-improver] Repeated no-op cycles on PR #<number> — possible livelock`
     and a body describing the PR's current `mergeable` / check state /
     `updatedAt` and the number of consecutive no-op runs. This replaces the
     `noop` call for this run — a stuck workflow must surface visibly.
   - Write the tracker back with `"escalated": true` so the escalation fires
     **at most once per PR**. Step 2's stuck/stale rules will force the PR to
     be closed and superseded within 24 hours regardless.
4. Otherwise write the updated tracker back and call the `noop` safe output
   with a message such as:
   > "No outstanding review comments found on PR #N. Auto-merge will trigger once
   > all checks pass."

**Stop** — do not run coverage analysis or create a new PR.

Whenever any other step pushes, closes, or creates a PR, reset the tracker to
`{"pr": <number>, "count": 0, "escalated": false}` — forward progress clears
the counter.

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

After creating the PR, reset the no-op tracker at
`/tmp/gh-aw/cache-memory/noop-tracker.json` to
`{"pr": <new number>, "count": 0, "escalated": false}` — a new PR is forward
progress.

### 16 — Nothing to do?

If coverage is already at 100% or all remaining uncovered branches are in
excluded files (`wizard_tty.rs`, `tests/`, etc.), call the `noop` safe output
with a message like:
> "Coverage analysis complete — no actionable uncovered branches found.
> Current branch coverage: XX.XX%."

This `noop` is not tracked by the Step 8 escalation counter — it concerns the
coverage analysis path, not an existing PR. If every run reaches this step
without ever opening a PR, that is a separate problem to report manually.
