---
date: 2026-03-24
researcher: Claude
git_commit: 00528c9368cc90b1c121c9e349f4d89ca8073c03
branch: main
repository: aipm
topic: "Bug: no_starter flag not forwarded to Claude Code adaptor — settings.json unconditionally enables starter plugin"
tags: [research, bug, init, wizard, settings-json, adaptor, no-starter]
status: complete
last_updated: 2026-03-24
last_updated_by: Claude
---

# Research: `no_starter` Flag Not Forwarded to Claude Code Adaptor

## Research Question

When running `aipm init` interactively and choosing "No" for the starter plugin,
why does `.claude/settings.json` still contain
`"starter-aipm-plugin@local-repo-plugins": true` in `enabledPlugins`?

## Summary

The `no_starter` flag flows correctly from the wizard through `Options` to
`scaffold_marketplace()`, which properly skips creating the starter plugin
directory and generates an empty `plugins` array in `marketplace.json`. However,
the `ToolAdaptor::apply()` trait method has no parameter for `no_starter`, so the
Claude Code adaptor unconditionally writes `enabledPlugins` with the starter
plugin entry in both the fresh-write and merge code paths.

## Detailed Findings

### 1. The `ToolAdaptor` Trait — No `no_starter` Parameter

The trait at [`mod.rs:17-30`](https://github.com/TheLarkInn/aipm/blob/00528c9368cc90b1c121c9e349f4d89ca8073c03/crates/libaipm/src/workspace_init/mod.rs#L17-L30)
defines:

```rust
pub trait ToolAdaptor {
    fn name(&self) -> &'static str;
    fn apply(&self, dir: &Path, fs: &dyn Fs) -> Result<bool, Error>;
}
```

The `apply` method takes only `dir` and `fs`. There is no mechanism to pass
`no_starter` or any `Options` to adaptors.

### 2. The `init()` Function — `no_starter` Passed to Scaffold but Not Adaptors

At [`mod.rs:100-124`](https://github.com/TheLarkInn/aipm/blob/00528c9368cc90b1c121c9e349f4d89ca8073c03/crates/libaipm/src/workspace_init/mod.rs#L100-L124):

```rust
if opts.marketplace {
    scaffold_marketplace(opts.dir, opts.no_starter, opts.manifest, fs)?;  // line 113 ✓
    // ...
    for adaptor in adaptors {
        if adaptor.apply(opts.dir, fs)? {  // line 117 — no_starter NOT passed ✗
            // ...
        }
    }
}
```

`opts.no_starter` is available at the call site (used one line above at line 113)
but never forwarded to the adaptor loop.

### 3. `scaffold_marketplace()` — Correctly Respects `no_starter`

At [`mod.rs:173-250`](https://github.com/TheLarkInn/aipm/blob/00528c9368cc90b1c121c9e349f4d89ca8073c03/crates/libaipm/src/workspace_init/mod.rs#L173-L250):

- Line 199: `generate_marketplace_json(no_starter)` produces an empty `plugins`
  array when `no_starter` is true.
- Lines 202-204: Early return skips the entire starter plugin directory tree.

### 4. Claude Code Adaptor — Unconditionally Enables Starter Plugin

#### Fresh-write path

At [`claude.rs:27-45`](https://github.com/TheLarkInn/aipm/blob/00528c9368cc90b1c121c9e349f4d89ca8073c03/crates/libaipm/src/workspace_init/adaptors/claude.rs#L27-L45),
the hardcoded JSON string at lines 39-41 includes:

```json
"enabledPlugins": {
    "starter-aipm-plugin@local-repo-plugins": true
}
```

#### Merge path

At [`claude.rs:93-98`](https://github.com/TheLarkInn/aipm/blob/00528c9368cc90b1c121c9e349f4d89ca8073c03/crates/libaipm/src/workspace_init/adaptors/claude.rs#L93-L98),
`merge_claude_settings()` unconditionally inserts
`"starter-aipm-plugin@local-repo-plugins": true` via `or_insert`.

Neither path consults any `no_starter` flag.

### 5. Data Flow Diagram

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

### 6. Test Coverage

The test `init_no_starter_still_configures_tools` at
[`mod.rs:958-977`](https://github.com/TheLarkInn/aipm/blob/00528c9368cc90b1c121c9e349f4d89ca8073c03/crates/libaipm/src/workspace_init/mod.rs#L958-L977)
verifies that with `no_starter: true`, `.claude/settings.json` is still created
and the starter plugin directory is absent. However, the test does not inspect the
*contents* of `settings.json` to verify that `enabledPlugins` omits the starter
plugin entry.

## Code References

- [`crates/libaipm/src/workspace_init/mod.rs:17-30`](https://github.com/TheLarkInn/aipm/blob/00528c9368cc90b1c121c9e349f4d89ca8073c03/crates/libaipm/src/workspace_init/mod.rs#L17-L30) — `ToolAdaptor` trait definition
- [`crates/libaipm/src/workspace_init/mod.rs:33-44`](https://github.com/TheLarkInn/aipm/blob/00528c9368cc90b1c121c9e349f4d89ca8073c03/crates/libaipm/src/workspace_init/mod.rs#L33-L44) — `Options` struct with `no_starter` field
- [`crates/libaipm/src/workspace_init/mod.rs:100-124`](https://github.com/TheLarkInn/aipm/blob/00528c9368cc90b1c121c9e349f4d89ca8073c03/crates/libaipm/src/workspace_init/mod.rs#L100-L124) — `init()` function; adaptor loop at line 117
- [`crates/libaipm/src/workspace_init/mod.rs:173-250`](https://github.com/TheLarkInn/aipm/blob/00528c9368cc90b1c121c9e349f4d89ca8073c03/crates/libaipm/src/workspace_init/mod.rs#L173-L250) — `scaffold_marketplace()` with `no_starter` early return
- [`crates/libaipm/src/workspace_init/mod.rs:452-484`](https://github.com/TheLarkInn/aipm/blob/00528c9368cc90b1c121c9e349f4d89ca8073c03/crates/libaipm/src/workspace_init/mod.rs#L452-L484) — `generate_marketplace_json()` conditional on `no_starter`
- [`crates/libaipm/src/workspace_init/adaptors/claude.rs:19-46`](https://github.com/TheLarkInn/aipm/blob/00528c9368cc90b1c121c9e349f4d89ca8073c03/crates/libaipm/src/workspace_init/adaptors/claude.rs#L19-L46) — `apply()` with hardcoded `enabledPlugins` at lines 39-41
- [`crates/libaipm/src/workspace_init/adaptors/claude.rs:49-106`](https://github.com/TheLarkInn/aipm/blob/00528c9368cc90b1c121c9e349f4d89ca8073c03/crates/libaipm/src/workspace_init/adaptors/claude.rs#L49-L106) — `merge_claude_settings()` unconditional insert at lines 93-98
- [`crates/libaipm/src/workspace_init/adaptors/mod.rs:13-15`](https://github.com/TheLarkInn/aipm/blob/00528c9368cc90b1c121c9e349f4d89ca8073c03/crates/libaipm/src/workspace_init/adaptors/mod.rs#L13-L15) — adaptor registry
- [`crates/libaipm/src/workspace_init/mod.rs:958-977`](https://github.com/TheLarkInn/aipm/blob/00528c9368cc90b1c121c9e349f4d89ca8073c03/crates/libaipm/src/workspace_init/mod.rs#L958-L977) — test that doesn't check `enabledPlugins` content

## Architecture Documentation

The init pipeline uses a two-phase design:
1. **Scaffolding** — `scaffold_marketplace()` creates the `.ai/` directory
   structure, receiving `no_starter` directly.
2. **Tool configuration** — The adaptor loop configures tool-specific files
   (e.g., `.claude/settings.json`) via the `ToolAdaptor` trait, which has no
   access to init options.

The disconnect is at the boundary between these two phases: the `ToolAdaptor`
trait was designed to only need `dir` and `fs`, but the `enabledPlugins` logic
in the Claude adaptor depends on whether the starter plugin was created.

## Where `no_starter` Does and Does Not Flow

| Location | `no_starter` consulted? |
|---|---|
| `scaffold_marketplace()` — `marketplace.json` content | Yes (line 199) |
| `scaffold_marketplace()` — early return before starter dir | Yes (line 202) |
| `init()` — adaptor loop execution | No (line 116-120) |
| `ToolAdaptor::apply()` trait signature | No parameter exists (line 29) |
| `claude::Adaptor::apply()` — fresh `settings.json` | No (lines 28-44) |
| `merge_claude_settings()` — merge path | No (lines 93-98) |

## Open Questions

1. Should the `ToolAdaptor::apply()` signature be extended to accept `&Options`
   (or at minimum `no_starter: bool`)?
2. Should the adaptor skip `enabledPlugins` entirely when `no_starter` is true,
   or should it write an empty `enabledPlugins` object?
3. Should the existing test `init_no_starter_still_configures_tools` be updated
   to assert that `enabledPlugins` does NOT contain the starter plugin entry?
