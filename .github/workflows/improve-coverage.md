---
description: >
  Coverage improver — runs every 15 minutes. Checks open coverage-improver PRs
  first for livelock (conflicting, failing, or stuck PRs are repaired or
  superseded instead of parking the workflow), then for Copilot review
  comments and applies any requested updates. If the PR needs no further
  changes, queues the build and enables auto-merge; repeated no-op cycles on
  a stuck PR escalate to an alert issue instead of reporting silent success.
  If no open PR exists, runs Rust branch coverage analysis, identifies one
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
  # Livelock guard outputs: repair or supersede a stuck coverage PR instead
  # of no-op'ing on it forever.
  update-pull-request:
    target: "*"
    required-title-prefix: "[coverage-improver]"
    title: false
    body: false
    update-branch: true
    max: 1
    # Use the PAT so the branch update emits a real `push` event and CI
    # re-runs on the repaired PR; GITHUB_TOKEN updates are suppressed.
    github-token: ${{ secrets.GH_AW_CI_TRIGGER_TOKEN }}
  close-pull-request:
    target: "*"
    required-title-prefix: "[coverage-improver]"
    max: 1
  # Escalation channel: a run that would otherwise be yet another silent
  # `noop` on a stuck PR files exactly one deduplicated alert issue.
  create-issue:
    max: 1
    title-prefix: "[coverage-improver] "
    labels: [coverage-improver, agentic-workflows]
    # The livelock alert would otherwise be re-filed every 15 minutes while
    # a PR stays stuck; drop repeats of an identical open alert title.
    deduplicate-by-title: true
  noop:
    report-as-issue: false
---

# Coverage Improver

You are an expert Rust developer improving branch coverage for this project.
The project enforces a strict **89% branch-coverage gate** (see `CLAUDE.md`).

## Goal

On each run, first check whether an open `[coverage-improver]` PR already
exists. If it does, verify it is still capable of making forward progress
(mergeable, checks healthy, recently active) and repair or supersede it if
not — then inspect it for unresolved Copilot review comments and act
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

- If **an open PR is found**, go to **Step 2** (livelock guard).
- If **no open PR is found**, go to **Step 6** (create a new PR).

### 2 — Verify the PR can actually merge (livelock guard)

**Never `noop` on a PR that is incapable of merging.** A `noop`-only run exits
`success`, so a stuck PR otherwise parks this workflow indefinitely while the
Actions dashboard shows green — this has already happened (a conflicting PR
livelocked the workflow for weeks of silent no-op runs).

Fetch the PR's current mergeability state:

```bash
gh pr view <number> --json mergeable,mergeStateStatus,statusCheckRollup,updatedAt
```

Decide from the result:

- **`mergeable: "CONFLICTING"`** — if the conflict is trivial (e.g.
  `Cargo.lock` churn or import ordering only), you may resolve it locally
  against `main`, verify the build, and push via `push-to-pull-request-branch`.
  Otherwise use the `close-pull-request` safe output to close the PR as
  superseded (say so in the closing comment), then go to **Step 6** to build a
  fresh replacement PR. **Do not `noop`.**
- **Failing checks with no actionable review comments** — diagnose the
  failures, fix them following the lint rules, verify locally (build, test,
  clippy, fmt), and push the fixes via `push-to-pull-request-branch`.
  **Do not `noop`.**
- **No checks reported at all** and `updatedAt` is older than **24 hours** —
  the PR is stuck (CI never started, so it can never merge). Close it as
  superseded via `close-pull-request` and go to **Step 6** to create a fresh
  PR. **Do not `noop`.**
- **`updatedAt` older than 48 hours** with no forward progress, regardless of
  other state — close as superseded via `close-pull-request` and go to
  **Step 6**. **Do not `noop`.**
- **Mergeable but `mergeStateStatus: "BEHIND"`** — use the
  `update-pull-request` safe output to update the branch with the latest
  `main` (`update-branch: true` is enabled, so no title/body change is made),
  then **stop**.
- Otherwise (mergeable, checks green or still running, recently active),
  continue to **Step 3**.

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

### 5 — Confirm the PR is ready

Reach this step only when the existing PR passed the Step 2 livelock guard and
has no actionable review comments.

1. **No-op escalation guard.** This workflow runs every 15 minutes, so a PR
   whose `updatedAt` is more than **24 hours** old has already absorbed ~96
   consecutive `noop` runs — silence indistinguishable from success on the
   Actions dashboard. Instead of `noop`, use the `create-issue` safe output to
   file a livelock alert:

   - **Title**: `Livelock guard: PR #N is stuck` (the `[coverage-improver] `
     title prefix is applied automatically)
   - **Body**: the PR number and link, its `mergeable` / `mergeStateStatus`
     values, how long it has been untouched, and a note that this workflow has
     been `noop`-ing on it. Request human triage.

   Then **stop**. Identical titles are dropped automatically
   (`deduplicate-by-title`), so an open alert is not re-filed every 15 minutes.

2. Otherwise (recently active, checks green or still running), call the `noop`
   safe output with a message such as:
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
