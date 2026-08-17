---
description: >
  Coverage improver — runs every 15 minutes. Checks open coverage-improver PRs
  for mergeability (conflicts, failing or absent checks, staleness) and repairs
  or supersedes unmergeable PRs before looking at Copilot review comments. If
  the PR needs no further changes, queues the build and enables auto-merge. If
  no open PR exists, runs Rust branch coverage analysis, identifies one
  uncovered branch, writes a test to cover it, and opens a PR explaining the
  scenario that the new test covers.
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
    max: 1
    target: "*"
    required-title-prefix: "[coverage-improver]"
  create-issue:
    max: 1
    labels: [coverage-improver, agentic-workflows]
    # Escalations for a stuck PR must not spam a new issue every 15 minutes —
    # deduplicate on the exact title so a still-open escalation is reused.
    deduplicate-by-title: true
  noop:
    report-as-issue: false
---

# Coverage Improver

You are an expert Rust developer improving branch coverage for this project.
The project enforces a strict **89% branch-coverage gate** (see `CLAUDE.md`).

## Goal

On each run, first check whether an open `[coverage-improver]` PR already
exists. If it does, verify the PR is actually capable of merging — a
conflicting, broken, or stale PR must never be treated as "in progress" — then
inspect it for unresolved Copilot review comments and act accordingly. If no PR
exists, find **one** uncovered branch, write the smallest possible test that
covers it, and open a PR.

**Never livelock.** Calling `noop` reports the run as a success, so parking on
an unmergeable PR looks healthy on the dashboard while the workflow
accomplishes nothing (this happened once for weeks — see issue #1391). Every
run must either move a PR toward merge, replace an unmergeable PR with fresh
work, or create a new PR. `noop` is only acceptable for a PR that is verifiably
healthy and recently active.

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
- If **no open PR is found**, go to **Step 9** (create a new PR).

### 2 — Assess PR mergeability and health

Before looking at review comments, confirm the PR can actually merge:

```bash
gh pr view <number> --json mergeable,mergeStateStatus,statusCheckRollup,updatedAt
```

Compute the PR's idle time as `now - updatedAt`. Then decide:

- **`mergeable` is `CONFLICTING`** → go to **Step 3** (repair or supersede).
- **Any entry in `statusCheckRollup` has conclusion `FAILURE`, `TIMED_OUT`, or
  `ACTION_REQUIRED`** → first read the unresolved review threads: if a thread
  already requests exactly the fix the failing check needs, go to **Step 7**
  (apply review comment updates). Otherwise go to **Step 4** (fix failing
  checks). Do **not** `noop` on a PR with failing checks.
- **No checks reported at all and the PR has been idle for more than 24
  hours** → the PR is stuck (checks never started and nothing is happening).
  Go to **Step 5** (close as superseded and escalate). Do **not** `noop`
  indefinitely on a check-less PR.
- **Idle for more than 48 hours with no forward progress** (no new commits, no
  new review activity, still unmerged) regardless of check state → go to
  **Step 5** (close as superseded and escalate).
- **Otherwise** (mergeable, checks passing or genuinely in progress, recently
  updated) → go to **Step 6** (inspect review comments).

If `mergeable` is `UNKNOWN`, re-fetch after a short wait; GitHub computes
mergeability lazily. If it stays `UNKNOWN`, treat the PR as suspect and apply
the staleness rules above based on `updatedAt`.

### 3 — Repair a conflicting PR, or close it as superseded

The PR has merge conflicts with `main`.

1. Check out the PR branch locally, fetch the latest `origin/main`, and rebase
   the branch onto it.
2. If the conflicts are **trivially resolvable** (e.g. adjacent-line edits in
   the same test module, `Cargo.lock` drift), resolve them, verify the code
   still builds and the test still passes (`cargo build --workspace` and
   `cargo test --workspace`), then push the rebased branch using the
   `push-to-pull-request-branch` safe output. After pushing, **stop** — CI will
   re-run and the next scheduled run takes over.
3. If the conflicts are **not trivially resolvable** (the covered branch moved,
   the test's subject code changed, or the rebase produces semantic conflicts),
   do not attempt heroic surgery. Close the PR using the
   `close-pull-request` safe output with a comment explaining it is superseded
   because it could not be merged cleanly, then go to **Step 9** (create a
   fresh PR against current `main`).

### 4 — Fix failing checks

The PR has failing checks that no review thread already covers.

1. Identify the failing check (`gh pr checks <number>`) and read its log
   (`gh run view --log-failed <run-id>`).
2. Determine the root cause and fix it on the PR branch, following all lint
   rules. Verify locally:

   ```bash
   cargo build --workspace
   cargo test --workspace
   cargo clippy --workspace -- -D warnings
   cargo fmt --check
   ```

3. Push the fix using the `push-to-pull-request-branch` safe output.

After pushing, **stop** — CI will re-run. Do **not** `noop` while checks are
red.

### 5 — Close a stuck or stale PR as superseded, and escalate

The PR cannot make progress on its own (stuck with no checks for >24h, or idle
for >48h).

1. Close the PR using the `close-pull-request` safe output with a comment
   stating why it was closed (stuck/stale) and that a fresh PR will replace it.
2. **Escalate visibly.** A stuck PR means earlier runs `noop`'d while nothing
   happened — and `noop`-only runs report `success`, so the stall was
   invisible. Emit the `create-issue` safe output with:
   - **Title**: `[gh-aw] Coverage Improver: closed stuck PR #<number> after repeated no-op cycles`
   - **Body**: the PR number, its last `updatedAt`, its mergeable/check state,
     and a note that the workflow superseded it with fresh work.
   Escalation issues are deduplicated by title, so repeated escalations about
   the same PR update the existing issue instead of spamming new ones.
3. Go to **Step 9** (create a fresh PR).

### 6 — Inspect for Copilot review comments

Read all open (unresolved) review threads on the existing PR. Focus on comments
left by Copilot or the `github-actions` bot that request code changes.

- If **there are unresolved review comments requesting code changes**,
  go to **Step 7** (apply the updates).
- If **there are no actionable review comments** (comments are resolved,
  informational only, or there are none at all),
  go to **Step 8** (confirm the PR is ready).

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

### 8 — Confirm the PR is ready

If there are no actionable review comments on the existing PR, and Step 2
confirmed it is mergeable with passing or genuinely in-progress checks:

1. **Staleness guard — check before you `noop`.** A healthy PR with auto-merge
   enabled merges within minutes of its checks passing. If the PR's
   `updatedAt` (fetched in Step 2) is more than **24 hours** old, this run is
   about to repeat a no-op on a PR that is not actually progressing — that is
   the livelock signature. Do **not** `noop`; go to **Step 5** (close as
   superseded and escalate) instead.
2. Otherwise, call the `noop` safe output with a message such as:
   > "No outstanding review comments found on PR #N. Auto-merge will trigger once
   > all checks pass."

**Stop** — do not run coverage analysis or create a new PR.

### 9 — Collect branch-level coverage

Reached when no open PR exists, or after a stuck/unmergeable PR was closed as
superseded in Step 3 or Step 5. Run a fresh coverage analysis:

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
