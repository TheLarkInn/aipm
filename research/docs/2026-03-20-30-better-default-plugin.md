---
date: 2026-03-20 07:38:32 PDT
researcher: Claude
git_commit: cf04f8826ae4c297f20d3366c1e33e28ab988888
branch: main
repository: aipm
topic: "GitHub Issue #30 — Create a better default plugin for aipm init"
tags: [research, codebase, init, plugin, starter, command, skill, agent, hook, marketplace]
status: complete
last_updated: 2026-03-20
last_updated_by: Claude
---

# Research: GitHub Issue #30 — Create a Better Default Plugin

## Research Question

GitHub Issue #30 requests that `aipm init` scaffold a more useful and informative default plugin containing:
1. A **command** (or skill with model invocation turned off) — a small TypeScript script that helps scaffold a new plugin in the `.ai` marketplace
2. A **sub-agent** scoped strictly to understand/scan code in the `.ai` marketplace
3. A **hook** providing basic logging after any model runs
4. All markdown in each file **under 50 lines**

## Summary

The current `aipm init` scaffolds a minimal "starter" plugin at `.ai/starter/` with a single generic `SKILL.md` and empty `agents/` and `hooks/` directories (`.gitkeep` only). Issue #30 asks to replace this with a more functional default plugin containing three real components — a scaffolding command/skill, a marketplace-scanning sub-agent, and a logging hook — all with concise markdown (< 50 lines per file).

## Detailed Findings

### Current Starter Plugin (What Exists Today)

The marketplace scaffolding is implemented in [`scaffold_marketplace()`](https://github.com/TheLarkInn/aipm/blob/cf04f8826ae4c297f20d3366c1e33e28ab988888/crates/libaipm/src/workspace_init/mod.rs#L165-L210).

Current `.ai/starter/` tree:

```
.ai/
  .gitignore
  starter/
    aipm.toml                    # [package] name="starter", type="composite"
    .claude-plugin/plugin.json   # Claude Code plugin metadata
    .mcp.json                    # empty MCP stub
    skills/hello/SKILL.md        # generic placeholder skill
    agents/.gitkeep              # empty
    hooks/.gitkeep               # empty
```

**Key observations:**
- The `SKILL.md` is a 12-line generic template with no real functionality (`workspace_init/mod.rs:238-252`)
- Agents and hooks directories have no content — just `.gitkeep` files
- The manifest at `workspace_init/mod.rs:212-227` declares only `skills = ["skills/hello/SKILL.md"]`
- The `plugin.json` at `workspace_init/mod.rs:229-236` mirrors basic package metadata

### Component 1: Command/Skill (Scaffold New Plugin)

**Issue requirement:** A command or skill with model invocation turned off, leveraging a small TypeScript script to scaffold a new plugin in `.ai`.

**Current codebase state:**
- Skills are declared in `[components] skills` as paths to `SKILL.md` files (`manifest/types.rs:118`)
- Commands are declared in `[components] commands` as legacy skill format (`manifest/types.rs:121`)
- SKILL.md files use YAML frontmatter; Claude Code supports `disable-model-invocation` in frontmatter per Copilot conventions (`research/docs/2026-03-16-copilot-agent-discovery.md:69`)
- The `[components] scripts` field (`manifest/types.rs:136`) exists for utility scripts that skills/hooks can reference
- AIPM treats SKILL.md and script files as opaque — it validates path existence only (`manifest/validate.rs:167-192`)

**What needs to change in `scaffold_marketplace()`:**
- Replace `skills/hello/SKILL.md` with a new skill (e.g., `skills/scaffold-plugin/SKILL.md`) that instructs the AI to run a TypeScript script for scaffolding
- Add a TypeScript script (e.g., `scripts/scaffold-plugin.ts`) referenced by the skill
- Update the manifest to list the new skill and script paths in `[components]`
- Ensure the skill frontmatter includes a `description` that clearly states when to invoke it

### Component 2: Sub-Agent (Marketplace Scanner)

**Issue requirement:** A sub-agent scoped strictly to understand and scan code in the `.ai` marketplace.

**Current codebase state:**
- Agents are declared in `[components] agents` as paths to agent markdown files (`manifest/types.rs:124`)
- Agent markdown files use YAML frontmatter with `name`, `description`, `tools`, `model` keys (`research/docs/2026-03-16-claude-code-defaults.md:160`)
- The `tools` frontmatter key is the scoping mechanism — restricts which tools an agent can use
- Currently `agents/.gitkeep` is the only file in the agents directory
- BDD quality lint (P2, unimplemented) will warn on missing `tools` declaration (`tests/features/guardrails/quality.feature:35-38`)

**What needs to change in `scaffold_marketplace()`:**
- Replace `agents/.gitkeep` with an actual agent markdown file (e.g., `agents/marketplace-scanner.md`)
- The agent should have `tools` frontmatter restricting capabilities (e.g., Read, Glob, Grep, LS only — no Edit/Write)
- The agent's instructions should be scoped to scanning/understanding `.ai/` directory contents
- Update the manifest to list the agent path in `[components] agents`

### Component 3: Hook (Post-Model Logging)

**Issue requirement:** A hook providing basic logging after any model runs.

**Current codebase state:**
- Hooks are declared in `[components] hooks` as paths to JSON files (`manifest/types.rs:127`)
- Claude Code hooks use `hooks.json` format with event-based lifecycle hooks (`research/docs/2026-03-16-claude-code-defaults.md:157`)
- Known hook events: `PreToolUse`, `PostToolUse` (and likely others from Claude Code's hook system)
- Currently `hooks/.gitkeep` is the only file in the hooks directory
- BDD quality lint (P2, unimplemented) will validate hook event names (`tests/features/guardrails/quality.feature:40-44`)

**What needs to change in `scaffold_marketplace()`:**
- Replace `hooks/.gitkeep` with `hooks/hooks.json` containing a post-model-run logging hook
- The hook should reference an event like `PostToolUse` or equivalent "after model runs" event
- Include a simple script or inline command for logging (e.g., append to a log file)
- Update the manifest to list `hooks = ["hooks/hooks.json"]` in `[components]`

### Manifest Changes Required

The current starter manifest (`workspace_init/mod.rs:212-227`) declares:

```toml
[components]
skills = ["skills/hello/SKILL.md"]
```

It needs to become something like:

```toml
[components]
skills = ["skills/scaffold-plugin/SKILL.md"]
agents = ["agents/marketplace-scanner.md"]
hooks = ["hooks/hooks.json"]
scripts = ["scripts/scaffold-plugin.ts"]
```

### 50-Line Markdown Constraint

All three markdown files (skill SKILL.md, agent .md, and any supporting docs) must be under 50 lines each. The current skill template is 12 lines, well under the limit.

### Validation Considerations

- `parse_and_validate()` with `base_dir` at `workspace_init/mod.rs:196-197` validates that all component paths exist on disk — all new files must be written **before** this validation call
- The current code already follows this pattern: `SKILL.md` is written at line 189, before validation at line 196
- New component files (agent .md, hooks.json, scripts/scaffold-plugin.ts) must also be written before validation

## Code References

- [`scaffold_marketplace()`](https://github.com/TheLarkInn/aipm/blob/cf04f8826ae4c297f20d3366c1e33e28ab988888/crates/libaipm/src/workspace_init/mod.rs#L165-L210) — Main function to modify
- [`generate_starter_manifest()`](https://github.com/TheLarkInn/aipm/blob/cf04f8826ae4c297f20d3366c1e33e28ab988888/crates/libaipm/src/workspace_init/mod.rs#L212-L227) — Manifest template to update
- [`generate_skill_template()`](https://github.com/TheLarkInn/aipm/blob/cf04f8826ae4c297f20d3366c1e33e28ab988888/crates/libaipm/src/workspace_init/mod.rs#L238-L252) — Skill template to replace
- [`generate_plugin_json()`](https://github.com/TheLarkInn/aipm/blob/cf04f8826ae4c297f20d3366c1e33e28ab988888/crates/libaipm/src/workspace_init/mod.rs#L229-L236) — Plugin JSON (may need description update)
- [`Components` struct](https://github.com/TheLarkInn/aipm/blob/cf04f8826ae4c297f20d3366c1e33e28ab988888/crates/libaipm/src/manifest/types.rs#L115-L143) — All valid component fields
- [`validate_component_paths()`](https://github.com/TheLarkInn/aipm/blob/cf04f8826ae4c297f20d3366c1e33e28ab988888/crates/libaipm/src/manifest/validate.rs#L167-L192) — Path existence validation
- [`PluginType` enum](https://github.com/TheLarkInn/aipm/blob/cf04f8826ae4c297f20d3366c1e33e28ab988888/crates/libaipm/src/manifest/types.rs#L208-L221) — Valid plugin types
- [Claude Code defaults research](https://github.com/TheLarkInn/aipm/blob/cf04f8826ae4c297f20d3366c1e33e28ab988888/research/docs/2026-03-16-claude-code-defaults.md#L153-L160) — SKILL.md, hooks.json, and agent .md format reference

## Architecture Documentation

### How `aipm init` Flows

1. CLI parses `Init` subcommand (`crates/aipm/src/main.rs:19-32`)
2. Defaults to marketplace-only if no flags (`main.rs:43-44`)
3. Loads tool adaptors — currently only Claude Code (`workspace_init/adaptors/mod.rs:13-15`)
4. Calls `workspace_init::init()` (`workspace_init/mod.rs:95-115`)
5. `scaffold_marketplace()` creates `.ai/starter/` tree (`mod.rs:165-210`) — **this is the function to modify**
6. Tool adaptors create/merge settings files (`adaptors/claude.rs:19-42`)
7. `InitResult` with action summaries returned to CLI for display (`main.rs:57-70`)

### Template Generation Pattern

All content is generated by dedicated `generate_*()` functions in `workspace_init/mod.rs`:
- `generate_workspace_manifest()` (lines 140-159)
- `generate_starter_manifest()` (lines 212-227)
- `generate_plugin_json()` (lines 229-236)
- `generate_skill_template()` (lines 238-252)
- `generate_mcp_stub()` (lines 254-256)

New content for issue #30 should follow this pattern — add `generate_agent_template()`, `generate_hook_template()`, and `generate_scaffold_script()` functions.

### Write-Before-Validate Ordering

Files must be written to disk before `parse_and_validate()` is called at line 196-197, because validation checks that component paths exist on disk. The existing code already establishes this pattern (SKILL.md written at line 189, validation at line 196).

## Historical Context (from research/)

- `research/docs/2026-03-16-claude-code-defaults.md` — Documents Claude Code's native file formats: SKILL.md with YAML frontmatter, hooks.json with lifecycle events (PreToolUse, PostToolUse), agent markdown with tools/model scoping
- `research/docs/2026-03-16-copilot-agent-discovery.md` — Documents `disable-model-invocation` frontmatter key for skills/agents (relevant for the "model invocation turned off" requirement)
- `research/docs/2026-03-16-aipm-init-workspace-marketplace.md` — Original research backing the workspace/marketplace init design
- `specs/2026-03-16-aipm-init-workspace-marketplace.md` — Specification for the init command
- `specs/2026-03-19-init-tool-adaptor-refactor.md` — Specification for the ToolAdaptor trait pattern used in init

## Related Research

- `research/docs/2026-03-16-claude-code-defaults.md` — Claude Code plugin format reference
- `research/docs/2026-03-16-copilot-agent-discovery.md` — Cross-tool agent discovery model
- `research/docs/2026-03-16-aipm-init-workspace-marketplace.md` — Init workspace/marketplace design

## Open Questions

1. **TypeScript script runtime:** The issue says "a small TypeScript script" for the scaffolding command — does the starter plugin need to include `ts-node`/`tsx` as a dependency, or should it use a different runtime (e.g., plain shell script, or `npx tsx`)?
2. **Hook event name:** The issue says "after any model runs" — is this `PostToolUse`, `Notification`, or a different Claude Code hook event? The research only documents `PreToolUse` and `PostToolUse`.
3. **Plugin naming:** Should the default plugin remain named "starter" or be renamed to something more descriptive?
4. **Script path in manifest:** The `[components] scripts` field exists in the schema but no current scaffold uses it — needs confirmation that scripts are validated the same way as other component paths.
