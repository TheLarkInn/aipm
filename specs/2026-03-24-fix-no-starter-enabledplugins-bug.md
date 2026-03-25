# Fix `no_starter` Flag Not Forwarded to Tool Adaptors

| Document Metadata      | Details                  |
| ---------------------- | ------------------------ |
| Author(s)              | selarkin                 |
| Status                 | Draft (WIP)              |
| Team / Owner           | aipm                     |
| Created / Last Updated | 2026-03-24 / 2026-03-24 |

## 1. Executive Summary

When a user runs `aipm init` and declines the starter plugin (via `--no-starter`
or the wizard's "No" answer), the `.ai/` directory is correctly created without
the starter plugin, but `.claude/settings.json` is still written with
`"starter-aipm-plugin@local-repo-plugins": true` in `enabledPlugins`. This
references a plugin that doesn't exist, creating a broken configuration. The fix
extends the `ToolAdaptor::apply()` trait to accept `no_starter`, allowing the
Claude Code adaptor to conditionally omit the `enabledPlugins` entry.

**Research reference:** [research/docs/2026-03-24-no-starter-enabledplugins-bug.md](../research/docs/2026-03-24-no-starter-enabledplugins-bug.md)

## 2. Context and Motivation

### 2.1 Current State

The `aipm init` pipeline has two phases:

1. **Scaffolding** — `scaffold_marketplace()` receives `no_starter` directly and
   correctly skips creating the starter plugin directory when `true`.
2. **Tool configuration** — The adaptor loop calls `adaptor.apply(dir, fs)` for
   each registered `ToolAdaptor`. The trait method has no parameter for
   `no_starter`, so the Claude Code adaptor unconditionally writes
   `enabledPlugins` with the starter plugin entry.

```
CLI --no-starter / Wizard "No"
       │
       ▼
  Options { no_starter: true }
       │
       ├──► scaffold_marketplace(no_starter=true)   ✓ respects flag
       │        ├─ marketplace.json: plugins = []   ✓
       │        └─ early return, no starter dir     ✓
       │
       └──► adaptor.apply(dir, fs)                  ✗ no_starter not passed
                └─ claude.rs writes settings.json
                   with enabledPlugins:
                   "starter-aipm-plugin@local-repo-plugins": true  ✗
```

### 2.2 The Problem

- **User Impact:** After `aipm init --no-starter`, `.claude/settings.json`
  references a plugin that doesn't exist, which may cause errors or warnings in
  Claude Code when it tries to load the non-existent starter plugin.
- **Inconsistency:** The `.ai/` directory correctly has no starter plugin and
  `marketplace.json` has an empty `plugins` array, but `settings.json` says the
  plugin is enabled — these two states contradict each other.

## 3. Goals and Non-Goals

### 3.1 Functional Goals

- [x] When `no_starter` is `true`, `.claude/settings.json` must NOT contain
  `"starter-aipm-plugin@local-repo-plugins"` in `enabledPlugins`.
- [x] When `no_starter` is `false` (the default), behavior is unchanged —
  `enabledPlugins` still includes the starter plugin entry.
- [x] The `extraKnownMarketplaces` section is always written regardless of
  `no_starter` — the marketplace registration is independent of which plugins
  are enabled.
- [x] The existing test `init_no_starter_still_configures_tools` is updated to
  verify `settings.json` contents.
- [x] All existing tests continue to pass.

### 3.2 Non-Goals (Out of Scope)

- [x] We will NOT pass the full `Options` struct to adaptors — only `no_starter`
  is needed. Passing the full struct would over-couple adaptors to init options
  and violate interface segregation. If more options are needed in the future, we
  can introduce an `AdaptorOptions` struct at that time.
- [x] We will NOT change how `enabledPlugins` works for the merge path when the
  starter plugin was previously enabled by the user — `or_insert` semantics are
  correct for the merge case (don't overwrite user's explicit choice).
- [x] We will NOT refactor the hardcoded JSON string in the fresh-write path to
  use `serde_json` — that's a separate cleanup concern.

## 4. Proposed Solution (High-Level Design)

### 4.1 Open Questions Resolution

The research document raised three open questions. Here are the decisions:

**Q1: Should `ToolAdaptor::apply()` accept `&Options` or just `no_starter: bool`?**

**Decision: Add `no_starter: bool` parameter.**

Passing the full `&Options` struct would couple every adaptor to all init
options, most of which are irrelevant (e.g., `workspace`, `manifest`, `dir`).
A single `bool` is the minimum information the adaptor needs. If future adaptors
need more options, we can introduce a purpose-built `AdaptorContext` struct then.

**Q2: Should the adaptor skip `enabledPlugins` entirely or write an empty object?**

**Decision: Skip the `enabledPlugins` key entirely when `no_starter` is true.**

An empty `"enabledPlugins": {}` adds no value — it's an unnecessary key that
would need to be populated later when the user installs their first plugin. The
`extraKnownMarketplaces` section (which registers the `.ai/` directory as a
marketplace source) should still always be written. The user can enable plugins
later via `aipm enable` or by editing `settings.json`.

**Q3: Should the test be updated to assert `enabledPlugins` content?**

**Decision: Yes.** The existing test `init_no_starter_still_configures_tools`
should be updated to read `settings.json`, parse it, and assert that
`enabledPlugins` either does not exist or does not contain the starter plugin
key. A new test should also verify the default path (no `--no-starter`) still
includes the `enabledPlugins` entry.

### 4.2 Key Components

| Component | Change | File |
|-----------|--------|------|
| `ToolAdaptor` trait | Add `no_starter: bool` parameter to `apply()` | `workspace_init/mod.rs` |
| `init()` function | Pass `opts.no_starter` to `adaptor.apply()` | `workspace_init/mod.rs` |
| `claude::Adaptor` | Conditionally omit `enabledPlugins` when `no_starter` | `adaptors/claude.rs` |
| `merge_claude_settings()` | Accept `no_starter`, skip starter entry when true | `adaptors/claude.rs` |
| Tests | Update assertions to verify `enabledPlugins` content | `mod.rs`, `claude.rs` |

## 5. Detailed Design

### 5.1 Trait Signature Change

**Before** (`mod.rs:29`):
```rust
fn apply(&self, dir: &Path, fs: &dyn Fs) -> Result<bool, Error>;
```

**After:**
```rust
fn apply(&self, dir: &Path, no_starter: bool, fs: &dyn Fs) -> Result<bool, Error>;
```

### 5.2 Call Site Change

**Before** (`mod.rs:117`):
```rust
if adaptor.apply(opts.dir, fs)? {
```

**After:**
```rust
if adaptor.apply(opts.dir, opts.no_starter, fs)? {
```

### 5.3 Claude Adaptor — Fresh-Write Path

**Before** (`claude.rs:19`):
```rust
fn apply(&self, dir: &Path, fs: &dyn Fs) -> Result<bool, Error> {
```

**After:**
```rust
fn apply(&self, dir: &Path, no_starter: bool, fs: &dyn Fs) -> Result<bool, Error> {
```

When `no_starter` is `true`, the fresh-write JSON string omits the
`enabledPlugins` block entirely. When `false`, behavior is unchanged.

The simplest approach: build the JSON with `serde_json::Map` (consistent with
the recent emitter refactor) and conditionally insert `enabledPlugins`.

Alternatively, keep two hardcoded string variants (one with, one without
`enabledPlugins`) to avoid adding serde construction to the hot path. Either
approach is acceptable; the conditional logic is the key change.

### 5.4 Claude Adaptor — Merge Path

**Before** (`claude.rs:24`):
```rust
return merge_claude_settings(&settings_path, fs);
```

**After:**
```rust
return merge_claude_settings(&settings_path, no_starter, fs);
```

Inside `merge_claude_settings()`, the `enabledPlugins` block at lines 92-98 is
wrapped in `if !no_starter { ... }`. The early-return check at lines 62-71 is
also updated: when `no_starter` is `true`, the "fully configured" check only
looks at `has_marketplace` (not `has_enabled`).

### 5.5 Test Updates

**Update `init_no_starter_still_configures_tools` (`mod.rs:958`):**
- After asserting `settings.json` exists, read and parse it.
- Assert `extraKnownMarketplaces.local-repo-plugins` is present (marketplace
  registration should always happen).
- Assert `enabledPlugins` either does not exist as a key or does not contain
  `"starter-aipm-plugin@local-repo-plugins"`.

**Update `claude_settings_created_fresh` (`claude.rs:127`):**
- This test calls `adaptor.apply(&tmp, &Real)` — update to
  `adaptor.apply(&tmp, false, &Real)` to pass `no_starter = false`.
- Existing assertions remain valid.

**Add `claude_settings_created_fresh_no_starter` (`claude.rs`):**
- Call `adaptor.apply(&tmp, true, &Real)`.
- Assert `extraKnownMarketplaces` is present.
- Assert `enabledPlugins` key does not exist in the parsed JSON.

**Update all other `claude.rs` tests** that call `adaptor.apply()` to pass
`false` for `no_starter` (preserving existing behavior).

**Update `merge_claude_settings` callers in tests** to pass `no_starter` where
the function is called directly.

## 6. Alternatives Considered

| Option | Pros | Cons | Decision |
|--------|------|------|----------|
| A: Pass `&Options` to `apply()` | Maximum future flexibility | Over-couples adaptors to all init options; most fields are irrelevant | Rejected |
| B: Pass `no_starter: bool` to `apply()` (selected) | Minimal change; clear intent; only the needed information | Requires updating all trait implementors | **Selected** — only one implementor exists (Claude) |
| C: Have `init()` conditionally skip the adaptor loop when `no_starter` | Zero trait changes | Wrong — tool configuration (marketplace registration) should still happen even without the starter plugin | Rejected |
| D: Post-process `settings.json` after adaptor runs to remove `enabledPlugins` | No trait changes | Fragile; breaks encapsulation; the adaptor should know what it writes | Rejected |

## 7. Cross-Cutting Concerns

### 7.1 Backward Compatibility

The `ToolAdaptor` trait is `pub` but only implemented within `libaipm` (one
implementor: `claude::Adaptor`). No external crates depend on this trait. The
signature change is a breaking API change to the trait but has zero external
consumers.

### 7.2 Future Adaptors

The comment at `adaptors/mod.rs:11-12` mentions future adaptors (Copilot CLI,
OpenCode, etc.). These would receive the `no_starter` parameter and can decide
independently whether their configuration needs it. Most future adaptors likely
don't reference a starter plugin, so they would simply ignore the parameter.

## 8. Implementation Plan

### Phase 1: Core Fix (Single PR)

- [ ] **Step 1:** Update `ToolAdaptor::apply()` signature to add `no_starter: bool`.
- [ ] **Step 2:** Update `init()` call site to pass `opts.no_starter`.
- [ ] **Step 3:** Update `claude::Adaptor::apply()` — conditionally omit
  `enabledPlugins` in fresh-write path when `no_starter` is `true`.
- [ ] **Step 4:** Update `merge_claude_settings()` — accept `no_starter`, skip
  starter plugin entry when `true`, update the "fully configured" early-return
  check.
- [ ] **Step 5:** Update all existing tests to pass `no_starter` argument.
- [ ] **Step 6:** Add new test: `claude_settings_created_fresh_no_starter`.
- [ ] **Step 7:** Update `init_no_starter_still_configures_tools` to assert
  `enabledPlugins` does NOT contain the starter plugin key.
- [ ] **Step 8:** Run full pipeline: `cargo fmt --check`, `cargo clippy`,
  `cargo test`, coverage >= 89%.

### Test Plan

- **Unit Tests:** All existing `claude.rs` tests updated + 1 new test for the
  `no_starter = true` fresh-write path.
- **Unit Tests:** `init_no_starter_still_configures_tools` updated with content
  assertions on `settings.json`.
- **E2E Tests:** Existing BDD scenarios for `--no-starter` flag.

## 9. Open Questions / Unresolved Issues

All three open questions from the research have been resolved in Section 4.1.
No remaining open questions.
