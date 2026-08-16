---
description: >
  Coverage improver — runs every 15 minutes. Checks open coverage-improver PRs
  for Copilot review comments and applies any requested updates. First verifies
  the open PR can still make progress (mergeable, checks running) and repairs
  or replaces it when it is stuck, so the workflow can never livelock on an
  unmergeable PR. If the PR needs no further changes, queues the build and
  enables auto-merge. If no open PR exists, runs Rust branch coverage analysis,
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
  # Status trail on the open coverage PR so repeated no-op cycles are
  # countable; older status comments from this workflow are hidden.
  add-comment:
    target: "*"
    max: 1
    hide-older-comments: true
    required-title-prefix: "[coverage-improver]"
  # Close a conflicting / stuck / stale coverage PR as superseded so a fresh
  # one can take its place instead of livelocking the workflow.
  close-pull-request:
    target: "*"
    required-title-prefix: "[coverage-improver]"
    max: 1
  # Escalation path: when the same PR no-ops repeatedly, file an issue so the
  # livelock is visible instead of reporting clean success.
  create-issue:
    max: 1
    title-prefix: "[coverage-improver] "
    labels: [coverage-improver, agentic-workflows]
    deduplicate-by-title: true
  noop:
    report-as-issue: false
---

# Coverage Improver

You are an expert Rust developer improving branch coverage for this project.
The project enforces a strict **89% branch-coverage gate** (see `CLAUDE.md`).

## Goal

On each run, first check whether an open `[coverage-improver]` PR already
exists. If it does, verify the PR can still make forward progress (mergeable,
checks running) and repair or replace it if it is stuck, then inspect it for
unresolved Copilot review comments and act accordingly. If no PR exists, find
**one** uncovered branch, write the smallest possible test that covers it, and
open a PR.

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
- If **no open PR is found**, go to **Step 6** (create a new PR).

### 2 — Assess the PR's mergeability and health

Before anything else, verify the PR is actually capable of merging:

```bash
gh pr view <number> --json mergeable,mergeStateStatus,statusCheckRollup,updatedAt,createdAt
```

Handle each case below. **Never call `noop` on an unhealthy PR** — a no-op on
a PR that cannot merge is how this workflow livelocks.

- **`mergeable` is `CONFLICTING`** — repair it:
  1. Check out the PR branch locally and merge `origin/main` into it.
  2. Resolve the conflicts (coverage-improver PRs usually conflict only with
     recently added tests — keep both sides).
  3. Verify `cargo build --workspace` and `cargo test --workspace` pass.
  4. Push the resolution with the `push-to-pull-request-branch` safe output,
     then **stop**.
  5. If the conflicts cannot be resolved cleanly, use `close-pull-request` to
     close the PR with a comment explaining it is superseded, then go to
     **Step 6** to create a fresh PR.
- **Failing checks and no actionable review comments** — the CI failure is the
  feedback: read the failing job logs, fix the failure, push with
  `push-to-pull-request-branch`, then **stop**. Do not `noop`.
- **No checks at all** (`statusCheckRollup` empty) **and the PR has not been
  updated for over 2 hours** — the PR is stuck; auto-merge can never fire
  without checks. Close it with `close-pull-request` ("superseded — no checks
  ever ran") and go to **Step 6**.
- **Stale** — `updatedAt` older than **24 hours** with the PR still unmerged:
  close it with `close-pull-request` ("superseded by a fresh run") and go to
  **Step 6**.

If the PR is healthy (mergeable, checks pending or passing), go to **Step 3**.

### 3 — Inspect for Copilot review comments

Read all open (unresolved) review threads on the existing PR. Focus on comments
left by Copilot or the `github-actions` bot that request code changes.

- If **there are unresolved review comments requesting code changes**,
  go to **Step 4** (apply the updates).
- If **there are no actionable review comments** (comments are resolved,
  informational only, or there are none at all),
  go to **Step 5** (confirm the PR is ready).

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

### 5 — Confirm the PR is ready (with no-op escalation)

The PR is healthy (Step 2) and has no actionable review comments. Before
calling `noop`, leave a visible trail so repeated no-op cycles are detectable —
silence must not be indistinguishable from success:

1. List the PR's comments and find the most recent status comment posted by
   this workflow (body contains `coverage-improver status:`). Compare the PR's
   current `updatedAt` with the one recorded in that comment:
   - **Same `updatedAt`** → nothing moved since the last run; this is another
     consecutive no-op: `consecutive = previous count + 1`.
   - **Different `updatedAt`** (or no previous status comment) → the PR
     changed; reset: `consecutive = 1`.
2. Use `add-comment` to post a status note on the PR:
   > `coverage-improver status: waiting — no outstanding review comments.
   > Auto-merge will trigger once all checks pass.
   > (updatedAt: <ISO-8601 timestamp>, consecutive no-ops: <N>)`

   Older status comments from this workflow are hidden automatically.
3. If `consecutive` is **greater than 8** (≈2 hours of 15-minute cycles with
   zero movement), the PR is livelocked: use `create-issue` with title
   `PR #<n> is not making progress` and a body describing the PR's state
   (mergeable, check rollup, last update) so a human intervenes. Duplicate
   escalations for the same PR are dropped automatically.
4. Otherwise call the `noop` safe output with a message such as:
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
