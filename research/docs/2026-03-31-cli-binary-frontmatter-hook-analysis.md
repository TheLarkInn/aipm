---
date: 2026-03-31 15:45:00 UTC
researcher: Claude
git_commit: 1b8483daae7b50608a93a114404330d1e235d222
branch: main
repository: aipm
topic: "Ground-truth analysis of Claude Code CLI and Copilot CLI frontmatter parsing, hook events, and validation"
tags: [research, codebase, lint, frontmatter, hooks, claude-code, copilot-cli, binary-analysis]
status: complete
last_updated: 2026-03-31
last_updated_by: Claude
---

# Research: CLI Binary Frontmatter & Hook Event Ground Truth

## Research Question

Can we extract the actual frontmatter parser, recognized fields, hook events, and validation logic from the Claude Code CLI and Copilot CLI binaries to keep aipm lint rules authoritative and compliant?

## Summary

**Yes.** Both CLIs were successfully analyzed — Claude Code v2.1.87 (compiled ELF with bundled JS) via `strings` extraction, and Copilot CLI v1.0.12 (Node.js) via direct source reading. The findings reveal significant discrepancies between our prior documentation and the actual implementations:

- **Hook events**: Claude Code supports **27 events** (not 22). Copilot CLI supports **10 events** (not the same set). Both tools have events the other doesn't.
- **Frontmatter fields**: Claude Code recognizes **16+ fields**. Copilot CLI validates via Zod schema with **5 fields** for skills and strict constraints (name max 64 chars, description max 1024 chars).
- **Frontmatter regex**: Both tools use nearly identical regex patterns but different YAML parsers.
- **Hook types**: Claude Code supports command + notify (plus agentic, HTTP, LLM prompt). Copilot supports command + prompt (sessionStart only).

This data should drive our lint rule definitions for accuracy.

---

## Detailed Findings

### 1. Frontmatter Parsing — Side by Side

| Aspect | Claude Code v2.1.87 | Copilot CLI v1.0.12 |
|--------|---------------------|---------------------|
| **Regex** | `/^---\s*\n([\s\S]*?)---\s*\n?/` | `/^---\s*\n([\s\S]*?)\n?---\s*(?:\n([\s\S]*))?$/` |
| **YAML parser** | Bundled `GQH()` function | Bundled `yaml` npm package |
| **Validation** | Type check (must be object, not array) | **Zod schema** with typed fields |
| **Unknown fields** | Silently accepted | Warned: `"unknown field(s) ignored: fieldA, fieldB"` |
| **Fallback on error** | Attempts escape/clean + re-parse | Returns `{kind: "error", message}` |
| **Error message** | `"Failed to parse YAML frontmatter in <path>: <error>"` | Zod error paths joined |

**Key insight**: Copilot CLI uses strict Zod schema validation and warns on unknown fields. Claude Code is more permissive, accepting any valid YAML mapping. Our lint rules should validate the **union** of both schemas to catch issues for either tool.

### 2. Recognized Skill Frontmatter Fields

| Field | Claude Code | Copilot CLI | Notes |
|-------|:-----------:|:-----------:|-------|
| `name` | Yes | Yes (max 64 chars, regex `^[a-zA-Z0-9][a-zA-Z0-9._\- ]*$`) | Copilot replaces spaces with hyphens |
| `description` | Yes | Yes (max 1024 chars) | Copilot auto-generates from body if absent |
| `allowed-tools` | Yes | Yes (string or string[]) | Copilot joins arrays with `", "` |
| `user-invocable` | Yes | Yes (boolean, default `true`) | Not on commands (always `true`) |
| `disable-model-invocation` | Yes | Yes (boolean, default `false`) | -- |
| `model` | Yes | No | Claude Code only |
| `effort` | Yes | No | Claude Code only |
| `context` | Yes | No | Claude Code only |
| `agent` | Yes | No | Claude Code only |
| `hooks` | Yes | No | Claude Code only |
| `shell` | Yes (validated: `bash`/`powershell`) | No | Claude Code only |
| `output-style` | Yes | No | Claude Code only |
| `aliases` | Yes | No | Claude Code only |
| `source` | Yes | No | Claude Code only |
| `argument-hint` | Yes | No | Claude Code only |
| `force-for-plugin` | Yes (output-styles only) | No | Claude Code only |
| `keep-coding-instructions` | Yes (output-styles only) | No | Claude Code only |

**Copilot-specific validation constraints:**
- `name`: Max 64 characters, must match `/^[a-zA-Z0-9][a-zA-Z0-9._\- ]*$/`
- `description`: Max 1024 characters
- `SKILL_CHAR_BUDGET`: 15000 chars (env `SKILL_CHAR_BUDGET`)

### 3. Hook Events — Complete Ground Truth

#### Claude Code (26 events, PascalCase)

| Event | Matcher Field | Matcher Values |
|-------|--------------|----------------|
| `PreToolUse` | `tool_name` | (all tool names) |
| `PostToolUse` | `tool_name` | (all tool names) |
| `PostToolUseFailure` | `tool_name` | (all tool names) |
| `Notification` | -- | -- |
| `SessionStart` | `source` | `startup`, `resume`, `clear`, `compact` |
| `Stop` | -- | -- |
| `StopFailure` | `error` | `rate_limit`, `authentication_failed`, `billing_error`, `invalid_request`, `server_error`, `max_output_tokens`, `unknown` |
| `SubagentStart` | `agent_type` | (dynamic) |
| `SubagentStop` | `agent_type` | (dynamic) |
| `PreCompact` | `trigger` | `manual`, `auto` |
| `PostCompact` | `trigger` | `manual`, `auto` |
| `SessionEnd` | `reason` | `clear`, `logout`, `prompt_input_exit`, `other` |
| `PermissionRequest` | `tool_name` | (all tool names) |
| `Setup` | `trigger` | `init`, `maintenance` |
| `TeammateIdle` | -- | -- |
| `TaskCreated` | -- | -- |
| `TaskCompleted` | -- | -- |
| `UserPromptSubmit` | -- | -- |
| `ToolError` | -- | -- |
| `Elicitation` | `mcp_server_name` | (dynamic) |
| `ElicitationResult` | `mcp_server_name` | (dynamic) |
| `ConfigChange` | `source` | `user_settings`, `project_settings`, `local_settings`, `policy_settings`, `skills` |
| `InstructionsLoaded` | `load_reason` | `session_start`, `nested_traversal`, `path_glob_match`, `include`, `compact` |
| `WorktreeCreate` | -- | -- |
| `WorktreeRemove` | -- | -- |
| `CwdChanged` | -- | -- |
| `FileChanged` | -- | -- |

#### Copilot CLI (10 events, camelCase)

| Event | Supports Matcher | Supports Prompt Type |
|-------|:----------------:|:-------------------:|
| `sessionStart` | No | Yes |
| `sessionEnd` | No | No |
| `userPromptSubmitted` | No | No |
| `preToolUse` | No | No |
| `postToolUse` | No | No |
| `errorOccurred` | No | No |
| `agentStop` | No | No |
| `subagentStop` | No | No |
| `subagentStart` | Yes | No |
| `preCompact` | Yes | No |

#### Copilot Legacy Name Mapping (PascalCase -> camelCase)

| Legacy Name | Maps To |
|-------------|---------|
| `SessionStart` | `sessionStart` |
| `SessionEnd` | `sessionEnd` |
| `UserPromptSubmit` | `userPromptSubmitted` |
| `PreToolUse` | `preToolUse` |
| `PostToolUse` | `postToolUse` |
| `PostToolUseFailure` | `errorOccurred` |
| `ErrorOccurred` | `errorOccurred` |
| `Stop` | `agentStop` |
| `SubagentStop` | `subagentStop` |
| `PreCompact` | `preCompact` |

#### Cross-Tool Event Comparison

| Event Concept | Claude Code Name | Copilot Name | Both? |
|---------------|-----------------|--------------|:-----:|
| Before tool use | `PreToolUse` | `preToolUse` | Yes |
| After tool use | `PostToolUse` | `postToolUse` | Yes |
| Tool use failure | `PostToolUseFailure` | `errorOccurred` | ~Yes (different name) |
| Session start | `SessionStart` | `sessionStart` | Yes |
| Session end | `SessionEnd` | `sessionEnd` | Yes |
| User prompt | `UserPromptSubmit` | `userPromptSubmitted` | ~Yes (different name) |
| Agent stop | `Stop` | `agentStop` | ~Yes (different name) |
| Subagent start | `SubagentStart` | `subagentStart` | Yes |
| Subagent stop | `SubagentStop` | `subagentStop` | Yes |
| Pre-compact | `PreCompact` | `preCompact` | Yes |
| Post-compact | `PostCompact` | -- | Claude only |
| Notification | `Notification` | -- | Claude only |
| Stop failure | `StopFailure` | -- | Claude only |
| Permission request | `PermissionRequest` | -- | Claude only |
| Setup | `Setup` | -- | Claude only |
| Teammate idle | `TeammateIdle` | -- | Claude only |
| Task created | `TaskCreated` | -- | Claude only |
| Task completed | `TaskCompleted` | -- | Claude only |
| Tool error | `ToolError` | -- | Claude only |
| Elicitation | `Elicitation` | -- | Claude only |
| Elicitation result | `ElicitationResult` | -- | Claude only |
| Config change | `ConfigChange` | -- | Claude only |
| Instructions loaded | `InstructionsLoaded` | -- | Claude only |
| Worktree create | `WorktreeCreate` | -- | Claude only |
| Worktree remove | `WorktreeRemove` | -- | Claude only |
| CWD changed | `CwdChanged` | -- | Claude only |
| File changed | `FileChanged` | -- | Claude only |

### 4. Hook Handler Types

| Type | Claude Code | Copilot CLI |
|------|:-----------:|:-----------:|
| `command` (shell) | Yes | Yes |
| `notify` (fire-and-forget) | Yes | No |
| `prompt` (LLM prompt) | Yes (referenced) | Yes (sessionStart only) |
| `http` (webhook) | Yes (referenced) | No |
| `agentic` (agent verifier) | Yes (referenced) | No |

#### Copilot Command Hook Schema (Zod)

| Field | Type | Required | Notes |
|-------|------|:--------:|-------|
| `type` | `"command"` | Yes (default) | -- |
| `bash` | `string` | One of bash/powershell/command | Shell command for bash |
| `powershell` | `string` | -- | Shell command for PowerShell |
| `command` | `string` | (legacy) | Copied to both bash and powershell if they're undefined |
| `cwd` | `string` | No | Working directory |
| `env` | `Record<string, string>` | No | Environment variables |
| `timeoutSec` | `number` (positive) | No | Timeout |
| `timeout` | `number` (positive, legacy) | No | Copied to timeoutSec |
| `matcher` | `string` (min 1) | No | Only on subagentStart and preCompact |

### 5. Plugin System Comparison

| Aspect | Claude Code | Copilot CLI |
|--------|-------------|-------------|
| **Plugin manifest** | `.claude-plugin/plugin.json` | `.claude-plugin/plugin.json` (same) |
| **Component fields** | `commands`, `agents`, `skills`, `outputStyles`, `hooks`, `settings` | Same discovery pattern |
| **Marketplace** | `extraKnownMarketplaces` in settings | `/plugin marketplace add/remove/list/browse` commands |
| **Install sources** | Git repos, URLs, local paths | GitHub repos, repo subdirs (`owner/repo:path`), URLs, marketplace |
| **Plugin ID format** | `<name>@<marketplace>` | `<name>@<marketplace>` (same) |
| **Skill sources** | `.claude/skills/`, personal `~/.claude/skills/` | `.github/skills/`, `.agents/skills/`, `.claude/skills/`, personal, custom dirs |
| **Skill char budget** | Not found | 15000 (env `SKILL_CHAR_BUDGET`) |

### 6. Copilot Built-in Agent Format

From `definitions/*.agent.yaml`:

| Field | Type | Example |
|-------|------|---------|
| `name` | string | `code-review` |
| `displayName` | string | `Code Review Agent` |
| `description` | string (multi-line) | Agent description |
| `model` | string | `claude-sonnet-4.5` |
| `tools` | string[] | `["*"]` or specific list |
| `contexts` | string[] (optional) | `["cli"]` |
| `promptParts` | object | `{includeX: boolean}` |
| `prompt` | template string | `{{cwd}}`, `{{grepToolName}}` |

Five built-in agents: `code-review`, `configure-copilot`, `explore`, `research`, `task`.

---

## Impact on aipm lint Rules

### hook/unknown-event Rule Must Be Updated

Our spec listed 22 events. The actual count is:
- **Claude Code**: 26 events (PascalCase)
- **Copilot CLI**: 10 events (camelCase) + legacy PascalCase mapping

The rule should:
1. Accept events valid for **either** tool (union of both sets)
2. Optionally warn when an event is tool-specific (e.g., `TeammateIdle` is Claude-only)
3. Accept both PascalCase and camelCase variants (Copilot normalizes legacy names)

### skill/missing-description and skill/missing-name Should Know Defaults

- Copilot auto-generates `description` from body content if absent (first 3 non-empty lines, max 1024 chars)
- Copilot derives `name` from parent directory if absent
- Claude Code also derives name from directory

This means "missing description" is only truly a problem for Claude Code (Copilot generates a fallback). The lint rule should still warn since the auto-generated description may be poor quality.

### skill/oversized Token Estimate Should Use Copilot's Budget

- Copilot CLI has `SKILL_CHAR_BUDGET = 15000` characters
- Our spec used `5000 tokens ≈ 20000 chars`
- Copilot's character budget is more restrictive

### New Potential Rules Discovered

| Rule ID | Source | What it catches |
|---------|--------|----------------|
| `skill/name-too-long` | Copilot | Skill name exceeds 64 characters |
| `skill/name-invalid-chars` | Copilot | Name doesn't match `/^[a-zA-Z0-9][a-zA-Z0-9._\- ]*$/` |
| `skill/description-too-long` | Copilot | Description exceeds 1024 characters |
| `skill/invalid-shell` | Claude Code | `shell` field not `bash` or `powershell` |
| `hook/tool-specific-event` | Both | Event is valid but only works in one tool |
| `hook/legacy-event-name` | Copilot | PascalCase event name should use camelCase |
| `hook/missing-command` | Copilot | Hook handler lacks `bash`, `powershell`, or `command` |

---

## Code References

### Claude Code CLI
- Binary: `/home/codespace/.local/share/claude/versions/2.1.87`
- Frontmatter regex: `_h8 = /^---\s*\n([\s\S]*?)---\s*\n?/`
- Shell validation: `haq = ["bash", "powershell"]`

### Copilot CLI
- SDK source: `/home/codespace/.agency/nodejs/node-v22.21.0-linux-x64/lib/node_modules/@github/copilot/sdk/index.js`
- App source: `/home/codespace/.agency/nodejs/node-v22.21.0-linux-x64/lib/node_modules/@github/copilot/app.js`
- Hook dispatch: `/home/codespace/.agency/nodejs/node-v22.21.0-linux-x64/lib/node_modules/@github/copilot/copilot-sdk/extension.js:3851-3861`
- Type definitions: `/home/codespace/.agency/nodejs/node-v22.21.0-linux-x64/lib/node_modules/@github/copilot/sdk/index.d.ts`
- Agent definitions: `/home/codespace/.agency/nodejs/node-v22.21.0-linux-x64/lib/node_modules/@github/copilot/definitions/*.agent.yaml`
- Frontmatter regex: `/^---\s*\n([\s\S]*?)\n?---\s*(?:\n([\s\S]*))?$/`

---

## Open Questions

1. **Should lint validate per-tool or cross-tool?** If a project uses both Claude Code and Copilot, events valid for one but not the other could cause silent failures in the other tool.

2. **How to keep event lists current?** Both tools ship as auto-updating binaries. The event lists will change. Options: (a) hard-code and update on each aipm release, (b) extract at lint-time from installed binaries, (c) maintain a schema file that can be updated independently.

3. **Should Copilot's Zod constraints become lint rules?** Copilot enforces name max 64 chars and description max 1024 chars. Claude Code doesn't enforce these. Should aipm lint warn about cross-tool compatibility?
