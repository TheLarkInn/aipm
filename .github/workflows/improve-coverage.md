---
description: >
  Coverage improver — runs every 15 minutes. Checks open coverage-improver PRs
  for Copilot review comments and applies any requested updates. Verifies an
  open PR can actually merge (conflicts, failing or missing checks, staleness)
  and escalates repeated no-ops instead of idling on a stuck PR. If the PR
  needs no further changes, queues the build and enables auto-merge. If no
  open PR exists, runs Rust branch coverage analysis, identifies one uncovered
  branch, writes a test to cover it, and opens a PR explaining the scenario
  that the new test covers.
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
  # Lets the agent close a coverage-improver PR that can never merge
  # (unresolvable conflicts, stale beyond recovery) so a fresh PR can replace
  # it. Restricted to the workflow's own title prefix.
  close-pull-request:
    target: "*"
    required-title-prefix: "[coverage-improver]"
    max: 1
  # Escalation channel for the no-op guard and the broken-auto-merge case:
  # instead of reporting a clean success, the workflow files an issue.
  # deduplicate-by-title + close-older-issues keep recurring escalations from
  # piling up; expires: false keeps the latest one open until a human acts.
  create-issue:
    max: 1
    title-prefix: "[coverage-improver-guard] "
    labels: [coverage-improver, agentic-workflows]
    deduplicate-by-title: true
    close-older-issues: true
    expires: false
  noop:
    report-as-issue: false
---

# Coverage Improver

You are an expert Rust developer improving branch coverage for this project.
The project enforces a strict **89% branch-coverage gate** (see `CLAUDE.md`).

## Goal

On each run, first check whether an open `[coverage-improver]` PR already
exists. If it does, verify the PR can actually make forward progress
(conflicts, failing or missing checks, staleness) before anything else, then
inspect it for unresolved Copilot review comments and act accordingly. If no
PR exists, find **one** uncovered branch, write the smallest possible test
that covers it, and open a PR.

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

### 2 — Assess mergeability before anything else

A previous incarnation of this workflow no-op'd indefinitely on a PR that
could never merge ([#887](https://github.com/TheLarkInn/aipm/pull/887)),
burning ~57 runs a day while every run reported `success`. **Never `noop` on
a PR that is stuck.**

Inspect the open PR's merge state with the GitHub pull-request tools — the
equivalent of:

```bash
gh pr view <n> --json mergeable,mergeStateStatus,statusCheckRollup,updatedAt
```

Evaluate the following rules **in order** and act on the **first** one that
matches. Only the last rule permits continuing to review-comment handling;
every other rule requires forward progress — never a `noop`:

1. **`mergeable: CONFLICTING` (or `mergeStateStatus: DIRTY`)** — the PR cannot
   merge. Try to repair it: check out the PR branch, rebase it onto the
   latest `main`, resolve any conflicts, verify build/tests/clippy/fmt all
   pass (the same commands as Step 4), and push with the
   `push-to-pull-request-branch` safe output. If the conflicts cannot be
   resolved with reasonable effort — for example the branch it covers is
   already covered on `main` — close the PR with the `close-pull-request`
   safe output (note that it is superseded and why), then go to **Step 6**
   to open a fresh PR. **Do not `noop`.**
2. **Failing checks and no actionable review comments** — the PR needs code
   changes, not patience. Diagnose the failing checks, fix the failures
   following all lint rules, verify locally, and push with
   `push-to-pull-request-branch`, then stop. **Do not `noop`.**
3. **No checks at all, and `updatedAt` is more than 2 hours ago** — CI never
   started and the PR is stuck (auto-merge can never fire). Rebase-push it
   onto `main` to retrigger CI; if that is not possible, close it with
   `close-pull-request` (note that it is superseded because checks never
   ran), then go to **Step 6**. **Do not `noop` indefinitely.**
4. **Stale: `updatedAt` is more than 48 hours old with no new commits, check
   progress, or comments** — no progress is happening regardless of what the
   fields claim. Close the PR with `close-pull-request` (note that it is
   superseded due to inactivity), then go to **Step 6**.
5. **`mergeable: MERGEABLE` with all checks green, but the PR is still
   open** — auto-merge should have fired and did not; this is a
   broken-merge condition no amount of waiting will fix. Escalate with the
   `create-issue` safe output (a title like "Coverage PR #N is mergeable
   with green checks but auto-merge never fired", a body describing the
   PR's merge state, check rollup, and `updatedAt`), then **stop**.
   **Do not `noop`.**
6. **Otherwise** (mergeable, checks pending or running normally, PR is
   fresh) — the PR is healthy and merely waiting on CI. Go to **Step 3**.

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

### 5 — Confirm the PR is ready (with the no-op escalation guard)

This step is reached only when Step 2 confirmed the PR is mergeable with
checks pending or in progress, and there are no actionable review comments —
the PR is waiting on CI or auto-merge and there is nothing to change. Before
calling `noop`, update the no-op guard state kept in cache memory so a
silently stuck PR cannot pass for a healthy one:

1. Read `/tmp/gh-aw/cache-memory/noop-guard.json` if it exists. It has the
   shape `{ "pr": <number>, "consecutive_noops": <count> }`.
2. If it refers to this same PR, increment `consecutive_noops`; otherwise
   start a new record for this PR with `consecutive_noops: 1`.
3. Write the updated record back to `/tmp/gh-aw/cache-memory/noop-guard.json`.
4. If `consecutive_noops` exceeds **8** (≈ 2 hours of runs at the 15-minute
   cadence), do **not** `noop`. A livelocked workflow that only emits `noop`
   still exits `success`, so repeated no-ops must surface visibly instead:
   - Use the `create-issue` safe output with a title like
     "Coverage Improver is no-op'ing on PR #N" and a body describing the
     PR's merge state, check rollup, and `updatedAt`.
   - Reset `consecutive_noops` to `0` in the state file.
   - Then make forward progress instead of idling: treat the PR as stuck
     per **Step 2** (push a fix, or close it as superseded and go to
     **Step 6**).

If the count is within the limit, call the `noop` safe output with a message
such as:
> "No outstanding review comments found on PR #N. Auto-merge will trigger once
> all checks pass."

**Stop** — do not run coverage analysis or create a new PR.

A `noop` is only legitimate here because Step 2 already verified the PR is
mergeable, has checks running, and is fresh — and the guard above bounds how
long that may continue unanswered. Any run that ends with a push, a closed
PR, or a newly created PR must reset the guard: delete
`/tmp/gh-aw/cache-memory/noop-guard.json` or write it with
`consecutive_noops: 0`.

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
