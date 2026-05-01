# Engine API Changelog

<!-- Entries are prepended (newest first) -->

## 2026-05-01 — Initial Schema Established

### claude v2.1.126 — initial schema established

| Field | Change |
|-------|--------|
| `tool_calls` | Baseline: 20 tools catalogued from `sdk-tools.d.ts` — Agent, Bash, TaskOutput, ExitPlanMode, FileEdit, FileRead, FileWrite, Glob, Grep, TaskStop, ListMcpResources, Mcp, NotebookEdit, ReadMcpResource, TodoWrite, WebFetch, WebSearch, AskUserQuestion, EnterWorktree, ExitWorktree |
| `settings_paths` | Baseline: `.claude/settings.json`, `.claude/settings.local.json`, `~/.claude/settings.json`, `.claude/CLAUDE.md`, `CLAUDE.md` |
| `folder_conventions` | Baseline: `.claude/`, `.claude/commands/`, `.claude/agents/` |
| `mcp_config` | Baseline: config at `.claude/mcp.json` or `~/.claude/mcp.json`; tool prefix `mcp__<server>__<tool>` |
| `output_styles` | Baseline: text, image, notebook, pdf, parts, file_unchanged |
| `size_limits` | Baseline: `head_limit` default 250, 0 = unlimited |
| `rules` | Baseline: file_path must be absolute; mode values: acceptEdits/auto/bypassPermissions/default/dontAsk/plan; isolation=worktree; model: sonnet/opus/haiku |
| `Agent` tool | Supports `isolation=worktree`, `mode`, `model` override; two output variants: `completed` and `async_launched` |
| `FileRead` tool | Five output variants: text, image, notebook, pdf, parts |
| `TodoWrite` tool | Items have status pending/in_progress/completed; chip labels max 12 chars |
| `service_tier` | Usage metadata includes standard/priority/batch tier |
| `cache_creation` | ephemeral_1h_input_tokens and ephemeral_5m_input_tokens in usage |

### copilot-cli — download failed (skipped)

Package `@github/copilot-cli` not found in npm registry. No API surface extracted.
Possible alternative package names to investigate: `@githubnext/github-copilot-cli`, `github-copilot-cli`.
