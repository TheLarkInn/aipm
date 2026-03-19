---
date: 2026-03-19 15:39:39 PDT
researcher: Claude Opus 4.6
git_commit: 8b950cfa9d30f3e24695dd724f9330ed776b001e
branch: main
repository: aipm
topic: "Refactor workspace init: remove vscode/copilot settings, introduce ToolAdaptor trait, Claude-only adaptor"
tags: [research, codebase, workspace-init, tool-adaptor, refactor, claude, copilot, vscode]
status: complete
last_updated: 2026-03-19
last_updated_by: Claude Opus 4.6
---

# Research: Init Tool Adaptor Refactor

## Research Question

Remove `.vscode/settings.json` and `.copilot/mcp-config.json` generation from marketplace init. Abstract tool-specific settings into a composable `ToolAdaptor` trait. Only build the Claude adaptor now; leave the API open for future adaptors (copilot-cli, opencode, etc.).

## Summary

The current `workspace_init.rs` hard-codes three tool integrations (`write_claude_settings`, `write_vscode_settings`, `write_copilot_config`) directly in the `init()` function. The refactor will extract tool-specific logic into a `ToolAdaptor` trait, delete the vscode and copilot implementations, and wrap the existing Claude settings logic in a `ClaudeAdaptor` struct. The `init()` function will accept a slice of adaptors, making tool integration composable. The `aipm.toml` workspace manifest is already gated behind the `--workspace` flag.

## Detailed Findings

### 1. Current `init()` Flow

The entry point is [`workspace_init::init()`](https://github.com/TheLarkInn/aipm/blob/8b950cfa9d30f3e24695dd724f9330ed776b001e/crates/libaipm/src/workspace_init.rs#L74-L98):

```rust
pub fn init(opts: &Options<'_>) -> Result<InitResult, Error> {
    let mut actions = Vec::new();

    if opts.workspace {
        init_workspace(opts.dir)?;                          // line 78
        actions.push(InitAction::WorkspaceCreated);
    }

    if opts.marketplace {
        scaffold_marketplace(opts.dir)?;                    // line 83
        actions.push(InitAction::MarketplaceCreated);

        if write_claude_settings(opts.dir)? {               // line 86
            actions.push(InitAction::ClaudeSettingsWritten);
        }
        if write_vscode_settings(opts.dir)? {               // line 89  ← DELETE
            actions.push(InitAction::VscodeSettingsWritten);
        }
        if write_copilot_config(opts.dir)? {                // line 92  ← DELETE
            actions.push(InitAction::CopilotConfigWritten);
        }
    }

    Ok(InitResult { actions })
}
```

### 2. Code to DELETE (vscode + copilot)

| Function | Lines | What it does |
|----------|-------|-------------|
| `write_vscode_settings()` | [325-337](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/libaipm/src/workspace_init.rs#L325-L337) | Creates or merges `.vscode/settings.json` with `chat.agentFilesLocations: [".ai"]` |
| `merge_vscode_settings()` | [339-375](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/libaipm/src/workspace_init.rs#L339-L375) | Merges `.ai` into existing vscode settings array |
| `write_copilot_config()` | [377-390](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/libaipm/src/workspace_init.rs#L377-L390) | Creates `.copilot/mcp-config.json` stub if missing |
| `InitAction::VscodeSettingsWritten` | [32](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/libaipm/src/workspace_init.rs#L32) | Enum variant |
| `InitAction::CopilotConfigWritten` | [34](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/libaipm/src/workspace_init.rs#L34) | Enum variant |

### 3. Code to KEEP and REFACTOR (claude settings → adaptor)

| Function | Lines | What it does |
|----------|-------|-------------|
| `write_claude_settings()` | [247-273](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/libaipm/src/workspace_init.rs#L247-L273) | Creates or merges `.claude/settings.json` with `extraKnownMarketplaces.local` pointing to `.ai` |
| `merge_claude_settings()` | [275-321](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/libaipm/src/workspace_init.rs#L275-L321) | Merges marketplace entry into existing Claude settings JSON without overwriting |

These two functions become the body of `ClaudeAdaptor::apply()`.

### 4. Code to KEEP UNCHANGED (core marketplace scaffolding)

| Function | Lines | What it does |
|----------|-------|-------------|
| `scaffold_marketplace()` | [148-193](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/libaipm/src/workspace_init.rs#L148-L193) | Creates `.ai/` directory tree, gitignore, starter plugin, skill template |
| `init_workspace()` | [104-121](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/libaipm/src/workspace_init.rs#L104-L121) | Creates `aipm.toml` workspace manifest (already behind `--workspace` flag) |
| `generate_*` functions | [123-239](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/libaipm/src/workspace_init.rs#L123-L239) | Template generators for manifests, skills, plugin.json, mcp stub |

### 5. CLI Entry Point

[`crates/aipm/src/main.rs:39-76`](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/aipm/src/main.rs#L39-L76):

```rust
// If neither flag is set, default to both
let (do_workspace, do_marketplace) =
    if !workspace && !marketplace { (true, true) } else { (workspace, marketplace) };
```

The CLI currently defaults to both `--workspace` and `--marketplace` when no flags are given. After the refactor, the CLI would construct the adaptor list and pass it to `init()`.

The match arms for `InitAction::VscodeSettingsWritten` (line 66) and `InitAction::CopilotConfigWritten` (line 69) need to be deleted and replaced with a generic `InitAction::ToolConfigured(name)` handler.

### 6. Tests to DELETE

| Test | Lines | Tests what |
|------|-------|-----------|
| `vscode_settings_created_fresh` | [570-580](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/libaipm/src/workspace_init.rs#L570-L580) | Fresh `.vscode/settings.json` creation |
| `vscode_settings_merge_existing` | [583-596](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/libaipm/src/workspace_init.rs#L583-L596) | Merging into existing vscode settings |
| `vscode_settings_skip_duplicate` | [599-612](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/libaipm/src/workspace_init.rs#L599-L612) | Skip if `.ai` already in locations |
| `copilot_config_created_fresh` | [615-622](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/libaipm/src/workspace_init.rs#L615-L622) | Fresh `.copilot/mcp-config.json` |
| `copilot_config_skip_existing` | [625-634](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/libaipm/src/workspace_init.rs#L625-L634) | Skip if already exists |

### 7. Tests to KEEP (move to claude adaptor tests)

| Test | Lines | Tests what |
|------|-------|-----------|
| `claude_settings_created_fresh` | [521-531](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/libaipm/src/workspace_init.rs#L521-L531) | Fresh `.claude/settings.json` creation |
| `claude_settings_merge_existing` | [534-552](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/libaipm/src/workspace_init.rs#L534-L552) | Merging into existing claude settings |
| `claude_settings_skip_if_present` | [555-567](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/libaipm/src/workspace_init.rs#L555-L567) | Skip if marketplace entry exists |

### 8. BDD Scenarios to DELETE

[`tests/features/manifest/workspace-init.feature`](https://github.com/TheLarkInn/aipm/blob/8b950cfa/tests/features/manifest/workspace-init.feature) lines 100-133:

| Scenario | Lines | Tests what |
|----------|-------|-----------|
| "Copilot VS Code settings point to .ai/" | 109-114 | `.vscode/settings.json` creation |
| "Copilot CLI MCP config stub is created" | 116-119 | `.copilot/mcp-config.json` creation |
| "Existing VS Code settings are merged not overwritten" | 127-132 | VS Code merge behavior |

### 9. BDD Scenarios to KEEP

| Scenario | Lines | Tests what |
|----------|-------|-----------|
| "Claude Code settings point to .ai/" | 102-107 | `.claude/settings.json` with `extraKnownMarketplaces` |
| "Existing Claude settings are not overwritten" | 121-125 | Claude merge behavior |

### 10. `aipm.toml` Flag Situation

`aipm.toml` is already behind the `--workspace` flag ([`main.rs:22-23`](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/aipm/src/main.rs#L22-L23)). However, `aipm init` with **no flags** defaults to `(true, true)` at [main.rs:43-44](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/aipm/src/main.rs#L43-L44), which creates both `aipm.toml` and `.ai/`. This default behavior is a separate decision from the adaptor refactor.

### 11. Module Structure

Currently everything lives in a single file: `crates/libaipm/src/workspace_init.rs` (674 lines). The adaptor trait and `ClaudeAdaptor` could either:

- **Stay in the same file** as a new section — simplest, keeps the module flat
- **Move to a submodule** `workspace_init/adaptors/claude.rs` — cleaner separation, allows future adaptors to be added as files without touching existing code

The existing module is publicly exported at [`crates/libaipm/src/lib.rs`](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/libaipm/src/lib.rs) as `pub mod workspace_init`.

## Code References

- [`crates/libaipm/src/workspace_init.rs`](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/libaipm/src/workspace_init.rs) — All init logic (674 lines)
- [`crates/aipm/src/main.rs:20-76`](https://github.com/TheLarkInn/aipm/blob/8b950cfa/crates/aipm/src/main.rs#L20-L76) — CLI `Init` command definition and dispatch
- [`tests/features/manifest/workspace-init.feature`](https://github.com/TheLarkInn/aipm/blob/8b950cfa/tests/features/manifest/workspace-init.feature) — BDD scenarios (133 lines)
- [`specs/2026-03-16-aipm-init-workspace-marketplace.md`](https://github.com/TheLarkInn/aipm/blob/8b950cfa/specs/2026-03-16-aipm-init-workspace-marketplace.md) — Original init spec

## Historical Context (from research/)

- [`research/docs/2026-03-16-aipm-init-workspace-marketplace.md`](https://github.com/TheLarkInn/aipm/blob/8b950cfa/research/docs/2026-03-16-aipm-init-workspace-marketplace.md) — Documents the init feature as designed, including tool settings integration for Claude Code, VS Code Copilot, and Copilot CLI
- [`research/docs/2026-03-16-copilot-agent-discovery.md`](https://github.com/TheLarkInn/aipm/blob/8b950cfa/research/docs/2026-03-16-copilot-agent-discovery.md) — Research on how Copilot discovers agents (informed the `.vscode/settings.json` and `.copilot/mcp-config.json` logic being removed)
- [`research/docs/2026-03-16-claude-code-defaults.md`](https://github.com/TheLarkInn/aipm/blob/8b950cfa/research/docs/2026-03-16-claude-code-defaults.md) — Research on Claude Code settings structure (informs the Claude adaptor)

## Related Research

- [`specs/2026-03-16-aipm-init-workspace-marketplace.md`](https://github.com/TheLarkInn/aipm/blob/8b950cfa/specs/2026-03-16-aipm-init-workspace-marketplace.md) — Original spec that defined all three tool integrations

## Open Questions

1. **Module structure**: Should the adaptor trait and `ClaudeAdaptor` stay in `workspace_init.rs` as a section, or be extracted to `workspace_init/adaptors/claude.rs`?
2. **Default `aipm init` behavior**: Should `aipm init` (no flags) still default to `--workspace --marketplace`, or should it change to `--marketplace` only now that the workspace manifest story is being deferred?
3. **Adaptor selection at CLI**: Should the CLI auto-detect which adaptors to use (e.g., check if `.claude/` exists), or always apply all registered adaptors, or accept a `--tool` flag?
