---
description: >
  Coverage improver — runs every 15 minutes. Checks open coverage-improver PRs
  for Copilot review comments and applies any requested updates. Open PRs are
  first assessed for mergeability and freshness: conflicting, failing, or stale
  PRs are repaired or closed as superseded so the workflow can never livelock on
  an unmergeable PR. If the PR needs no further changes, queues the build and
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
  update-pull-request:
    target: "*"
    required-title-prefix: "[coverage-improver]"
    title: false
    body: false
    update-branch: true
    max: 1
  close-pull-request:
    target: "*"
    required-title-prefix: "[coverage-improver]"
    max: 1
  create-issue:
    title-prefix: "[coverage-improver]"
    labels: [coverage-improver, automation]
    max: 1
  noop:
    report-as-issue: false
---

# Coverage Improver

You are an expert Rust developer improving branch coverage for this project.
The project enforces a strict **89% branch-coverage gate** (see `CLAUDE.md`).

## Goal

On each run, first check whether an open `[coverage-improver]` PR already
exists. If it does, assess whether it can actually merge — a conflicting,
failing, or stale PR must be repaired or replaced, never waited on — then
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

### 2 — Assess mergeability and freshness

Before looking at review comments, check whether the PR is actually capable of
making forward progress:

```bash
gh pr view <number> --json mergeable,mergeStateStatus,statusCheckRollup,updatedAt
```

Evaluate the following rules **in order**:

1. **`mergeable` is `CONFLICTING`** — first try a server-side branch update
   with the `update-pull-request` safe output (`update-branch`), which merges
   the latest `main` into the PR branch. If the PR is still `CONFLICTING`
   afterwards, check out the PR branch locally, rebase it onto `origin/main`,
   resolve the conflicts, verify `cargo build --workspace`,
   `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, and
   `cargo fmt --check` all pass, and push the result with
   `push-to-pull-request-branch`. If the conflicts cannot be resolved cleanly,
   close the PR as superseded with `close-pull-request` (leave a comment
   explaining why) and go to **Step 6** to open a fresh PR.
   **Do not `noop` on a conflicting PR.**

2. **Failing checks and no actionable review comments** — reproduce the
   failure locally, fix it (build, tests, clippy, and fmt must all pass), and
   push the fix with `push-to-pull-request-branch`. **Do not `noop`.**

3. **No checks reported at all, or checks stuck pending, with `updatedAt`
   older than 4 hours** — the PR is wedged (auto-merge never fired, or CI
   never ran). Close it as superseded with `close-pull-request` and go to
   **Step 6** to start fresh. **Do not `noop` indefinitely.**

4. **`updatedAt` older than 48 hours** regardless of any other state — the PR
   is stale. Close it as superseded with `close-pull-request` and go to
   **Step 6**.

5. **Otherwise** (mergeable, checks running or passing, recently updated) —
   go to **Step 3** (inspect review comments).

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

If there are no actionable review comments on the existing PR, and Step 2
found it mergeable with no failing or stuck checks:

1. **Escalate repeated no-ops.** Check how long the PR has been idle. If
   `updatedAt` is older than **24 hours**, dozens of consecutive runs have
   already `noop`'d on this PR with no forward progress — silence must not be
   indistinguishable from success. Instead of `noop`:

   - Search for an existing open escalation issue:
     `gh issue list --label coverage-improver --state open --search "stalled"`.
   - If none exists for this PR, use the `create-issue` safe output to open an
     escalation issue titled `PR #<number> stalled — repeated no-op cycles`
     describing the PR's `mergeable`, `mergeStateStatus`, failing or pending
     checks, and last update time. Then stop.
   - If an escalation issue already covers this PR, `noop` normally — the
     stall is already surfaced.

2. Otherwise, call the `noop` safe output with a message such as:
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
