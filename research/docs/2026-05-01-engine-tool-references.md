---
date: 2026-05-01 23:30:00 UTC
researcher: Sean Larkin
git_commit: 0f4e837c0e3ba30ad34827197fd54c0c6a9a7348
branch: main
repository: aipm
topic: "Engine tool-name reference catalogs for `agent/valid-tool-name` lint rule"
tags: [research, engines, claude, copilot-cli, lint, tool-names, issue-697]
status: complete
last_updated: 2026-05-01
last_updated_by: Sean Larkin
---

# Engine Tool-Name References (Claude + copilot-cli)

## Research Context

Issue [#697](https://github.com/TheLarkInn/aipm/issues/697) defines a new lint rule
`agent/valid-tool-name` that validates tool-name references in agent files against the
engine(s) the project targets via the `[package].engines` field (issue
[#510](https://github.com/TheLarkInn/aipm/issues/510)). The owner-comment on #697
specifies two authoritative documentation URLs to use as the source of truth for tool
names *until the binary trace/analysis matures*:

- copilot-cli: <https://docs.github.com/en/copilot/reference/custom-agents-configuration#tool-aliases>
- claude: <https://code.claude.com/docs/en/tools-reference>

This document captures the canonical tool-name catalogs from both pages and the
cross-engine intersection that the lint rule will compare against.

> **Snapshot date: 2026-05-01.** Both pages may change. When the
> `.github/workflows/reverse-binary-analysis.md` workflow successfully populates
> `research/engine-api-schema.json`, that file becomes authoritative and this doc
> becomes secondary. Until then, treat this snapshot as the lint rule's source data.

---

## copilot-cli Tool Aliases

**Source**: GitHub Copilot — Custom agents configuration / Tool aliases.

### Primary tool aliases

| Primary alias | Compatible aliases                              | Cloud-agent mapping            | Default availability                              | Description                                                |
|---------------|-------------------------------------------------|--------------------------------|---------------------------------------------------|------------------------------------------------------------|
| `execute`     | `shell`, `Bash`, `powershell`                   | `bash` or `powershell`         | Default                                           | Execute a command in the appropriate shell for the OS      |
| `read`        | `Read`, `NotebookRead`                          | `view`                         | Default                                           | Read file contents                                         |
| `edit`        | `Edit`, `MultiEdit`, `Write`, `NotebookEdit`    | `str_replace`, `str_replace_editor` | Default                                      | Allow LLM to edit (exact arguments vary)                   |
| `search`      | `Grep`, `Glob`                                  | `search`                       | Default                                           | Search for files or text in files                          |
| `agent`       | `custom-agent`, `Task`                          | "Custom agent" tools           | Default                                           | Invoke a different custom agent to accomplish a task       |
| `web`         | `WebSearch`, `WebFetch`                         | n/a (cloud agent)              | Default (local); **not in cloud agent**           | Fetch content from URLs and perform web search             |
| `todo`        | `TodoWrite`                                     | n/a                            | Default (VS Code); **not supported in cloud agent today** | Create and manage structured task lists           |

### Out-of-the-box MCP servers (referenced as `<server>/*` or `<server>/<tool>`)

| Server name  | Default availability                                                  | Description                                                |
|--------------|-----------------------------------------------------------------------|------------------------------------------------------------|
| `github`     | Read-only tools available by default; token scoped to source repo     | All read-only tools from GitHub MCP server                 |
| `playwright` | Available by default; localhost-only access                           | All Playwright tools for web automation                    |

### Configuration syntax (verbatim YAML examples from the page)

```yaml
tools: ['tool-a', 'tool-b', 'custom-mcp/tool-1']
tools: ["read", "search", "edit"]
tools: ["*"]
tools: []
```

### Full set of names valid in a copilot-cli `tools:` array (per docs)

- **Primary aliases**: `execute`, `read`, `edit`, `search`, `agent`, `web`, `todo`
- **Compatible aliases**: `shell`, `Bash`, `powershell`, `Read`, `NotebookRead`, `Edit`,
  `MultiEdit`, `Write`, `NotebookEdit`, `Grep`, `Glob`, `custom-agent`, `Task`,
  `WebSearch`, `WebFetch`, `TodoWrite`
- **MCP server prefixes**: `github/*`, `github/<tool>`, `playwright/*`, `playwright/<tool>`
- **Wildcards**: `"*"` (all) and `[]` (none)

---

## Claude Tools Reference

**Source**: Claude Code — Tools reference. Per the docs: tool names are the *exact strings
you use in permission rules, subagent tool lists, and hook matchers* — case-sensitive.

### Built-in tools (33 total)

| Tool name              | Permission required | Description                                                                                                | Gating                                                                                                               |
|------------------------|---------------------|------------------------------------------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------|
| `Agent`                | No                  | Spawns a subagent with its own context window                                                              | Default                                                                                                              |
| `AskUserQuestion`      | No                  | Asks multiple-choice questions to gather requirements or clarify ambiguity                                 | Default                                                                                                              |
| `Bash`                 | Yes                 | Executes shell commands in your environment                                                                | Default                                                                                                              |
| `CronCreate`           | No                  | Schedules a recurring or one-shot prompt within the current session                                        | Default                                                                                                              |
| `CronDelete`           | No                  | Cancels a scheduled task by ID                                                                             | Default                                                                                                              |
| `CronList`             | No                  | Lists all scheduled tasks in the session                                                                   | Default                                                                                                              |
| `Edit`                 | Yes                 | Makes targeted edits to specific files                                                                     | Default                                                                                                              |
| `EnterPlanMode`        | No                  | Switches to plan mode to design an approach before coding                                                  | Default                                                                                                              |
| `EnterWorktree`        | No                  | Creates an isolated git worktree and switches into it                                                      | Default; **not available to subagents**                                                                              |
| `ExitPlanMode`         | Yes                 | Presents a plan for approval and exits plan mode                                                           | Default                                                                                                              |
| `ExitWorktree`         | No                  | Exits a worktree session and returns to the original directory                                             | Default; **not available to subagents**                                                                              |
| `Glob`                 | No                  | Finds files based on pattern matching                                                                      | Default                                                                                                              |
| `Grep`                 | No                  | Searches for patterns in file contents                                                                     | Default                                                                                                              |
| `ListMcpResourcesTool` | No                  | Lists resources exposed by connected MCP servers                                                           | Default                                                                                                              |
| `LSP`                  | No                  | Code intelligence via language servers                                                                     | **Inactive until a code-intelligence plugin is installed**                                                           |
| `Monitor`              | Yes                 | Runs a command in the background and feeds output lines back to Claude                                     | Requires Claude Code v2.1.98+; not on Bedrock/Vertex/Foundry; disabled when telemetry/non-essential traffic disabled |
| `NotebookEdit`         | Yes                 | Modifies Jupyter notebook cells                                                                            | Default                                                                                                              |
| `PowerShell`           | Yes                 | Executes PowerShell commands natively                                                                      | Gated by `CLAUDE_CODE_USE_POWERSHELL_TOOL=1`; auto-enabled on Windows w/o Git Bash; opt-in on Linux/macOS/WSL        |
| `Read`                 | No                  | Reads the contents of files                                                                                | Default                                                                                                              |
| `ReadMcpResourceTool`  | No                  | Reads a specific MCP resource by URI                                                                       | Default                                                                                                              |
| `SendMessage`          | No                  | Sends a message to an agent-team teammate or resumes a subagent                                            | Only available when `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`                                                         |
| `Skill`                | Yes                 | Executes a skill within the main conversation                                                              | Default                                                                                                              |
| `TaskCreate`           | No                  | Creates a new task in the task list                                                                        | Default (interactive sessions)                                                                                       |
| `TaskGet`              | No                  | Retrieves full details for a specific task                                                                 | Default (interactive sessions)                                                                                       |
| `TaskList`             | No                  | Lists all tasks with their current status                                                                  | Default (interactive sessions)                                                                                       |
| `TaskOutput`           | No                  | (Deprecated) Retrieves output from a background task                                                       | **Deprecated**                                                                                                       |
| `TaskStop`             | No                  | Kills a running background task by ID                                                                      | Default                                                                                                              |
| `TaskUpdate`           | No                  | Updates task status, dependencies, details, or deletes tasks                                               | Default (interactive sessions)                                                                                       |
| `TeamCreate`           | No                  | Creates an agent team with multiple teammates                                                              | Only available when `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`                                                         |
| `TeamDelete`           | No                  | Disbands an agent team and cleans up teammate processes                                                    | Only available when `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`                                                         |
| `TodoWrite`            | No                  | Manages the session task checklist                                                                         | Available in non-interactive mode and Agent SDK; interactive sessions use `TaskCreate`/`TaskGet`/`TaskList`/`TaskUpdate` |
| `ToolSearch`           | No                  | Searches for and loads deferred tools                                                                      | Gated by tool search being enabled                                                                                   |
| `WebFetch`             | Yes                 | Fetches content from a specified URL                                                                       | Default                                                                                                              |
| `WebSearch`            | Yes                 | Performs web searches                                                                                      | Default                                                                                                              |
| `Write`                | Yes                 | Creates or overwrites files                                                                                | Default                                                                                                              |

> Per the Claude docs: "Your exact tool set depends on your provider, platform, and
> settings." The 33-tool list is the documented upper bound; some platforms expose
> fewer.

---

## Cross-Engine Comparison

The matching rule used below: a tool is "shared" if its exact-cased name appears in the
Claude reference AND either (a) appears as a primary or compatible alias in
copilot-cli's table, or (b) is listed as an out-of-the-box MCP server prefix.

### Shared / overlapping tools (the "valid subset")

These Claude tool names ARE explicitly listed as copilot-cli compatible aliases — they
will pass the lint when `engines` includes both `claude` and `copilot-cli`:

| Claude name      | copilot-cli primary alias | Match basis                           |
|------------------|---------------------------|---------------------------------------|
| `Bash`           | `execute`                 | listed as compatible alias of `execute` |
| `Read`           | `read`                    | listed as compatible alias of `read`  |
| `NotebookEdit`   | `edit`                    | listed as compatible alias of `edit`  |
| `Edit`           | `edit`                    | listed as compatible alias of `edit`  |
| `Write`          | `edit`                    | listed as compatible alias of `edit`  |
| `Grep`           | `search`                  | listed as compatible alias of `search` |
| `Glob`           | `search`                  | listed as compatible alias of `search` |
| `WebSearch`      | `web`                     | listed as compatible alias of `web`   |
| `WebFetch`       | `web`                     | listed as compatible alias of `web`   |
| `TodoWrite`      | `todo`                    | listed as compatible alias of `todo`  |

**Shared count: 10 tools** (PascalCase strings recognized by both engines).

Additionally, copilot-cli lists `NotebookRead`, `MultiEdit`, and `Task` as compatible
aliases, but **none of those exact strings appear in the Claude tools table** (Claude
has `NotebookEdit` but not `NotebookRead`; has `Edit` but not `MultiEdit`; uses `Agent`
rather than `Task` for subagent dispatch). They are copilot-cli-only from a strict
case-sensitive standpoint.

### Claude-only tools (warn when `engines` includes `copilot-cli`)

Tools in the Claude reference but with no exact name/alias match in copilot-cli's table:

`Agent`, `AskUserQuestion`, `CronCreate`, `CronDelete`, `CronList`, `EnterPlanMode`,
`EnterWorktree`, `ExitPlanMode`, `ExitWorktree`, `ListMcpResourcesTool`, `LSP`,
`Monitor`, `PowerShell`, `ReadMcpResourceTool`, `SendMessage`, `Skill`, `TaskCreate`,
`TaskGet`, `TaskList`, `TaskOutput`, `TaskStop`, `TaskUpdate`, `TeamCreate`,
`TeamDelete`, `ToolSearch`

**Claude-only count: 25 tools** (case-sensitive strict match).

`Agent` and `PowerShell` differ in copilot-cli only by case (`agent` and `powershell`).
If the lint rule does case-insensitive matching, those two move into the shared bucket.

### copilot-cli-only tools (warn when `engines` includes `claude`)

Names that appear in copilot-cli's docs but not in Claude's tools table:

- **Primary lowercase aliases** (none of these exact strings appear in Claude's table):
  `execute`, `read`, `edit`, `search`, `agent`, `web`, `todo`
- **Compatible aliases not present in Claude's table**:
  `shell`, `powershell`, `NotebookRead`, `MultiEdit`, `custom-agent`, `Task`
- **Cloud-agent mapping names** (likely internal, not user-facing):
  `bash`, `view`, `str_replace`, `str_replace_editor`
- **Out-of-the-box MCP server prefixes**:
  `github` (and `github/*`, `github/<tool>`), `playwright` (and `playwright/*`, `playwright/<tool>`)

**copilot-cli-only count: 19 distinct strings** (15 if cloud-agent-internal mapping
names are excluded as not user-facing).

### Summary table

| Engine      | Total documented names                                  | Shared (strict case) | Engine-exclusive |
|-------------|---------------------------------------------------------|----------------------|------------------|
| Claude      | 33 built-in tools                                       | 10                   | 25               |
| copilot-cli | 7 primary + 16 aliases + 2 MCP servers = 25 strings     | 10                   | 19               |

---

## Implementation Notes for `agent/valid-tool-name`

1. **Matching strategy must be alias-aware.** A naive string-equality check would flag
   `Bash` as Claude-only when copilot-cli explicitly accepts it. Build two sets:
   - `claude_valid`: the 33 PascalCase names from Claude's table
   - `copilot_valid`: union of {primary aliases} ∪ {compatible aliases} ∪
     {`github`, `github/*`, `playwright`, `playwright/*`} ∪ {wildcard `*`}

2. **Wildcard handling.** copilot-cli supports `tools: ["*"]` (all tools) and
   `tools: []` (no tools) per the YAML examples. Treat `*` as always valid. Claude's
   docs do not show a `*` syntax for tool lists; subagent tool lists in Claude expect
   explicit names.

3. **MCP-prefixed tools.** copilot-cli supports `custom-mcp/tool-1`-style entries.
   Either skip validation for any tool name containing `/` (treat as MCP-server-scoped
   and out of scope for this rule), or parse the prefix and validate against the known
   out-of-the-box servers (`github`, `playwright`).

4. **Case sensitivity is real.** copilot-cli's table demonstrates that `execute` and
   `Bash` are both valid (lowercase primary + PascalCase alias). The Claude page says
   the names are the "exact strings" — so `bash` lowercase would NOT be valid in a
   Claude permission rule even though `Bash` is. Preserve case in error messages.

5. **Gated tools should still be valid names.** Tools like `LSP`, `Monitor`,
   `PowerShell`, `SendMessage`, `TeamCreate`, `TeamDelete`, `ToolSearch` are gated by
   env-vars / versions / plugins but are documented canonical names. Accept them;
   gating is a runtime concern, not a name-validity concern.

6. **Deprecated tools.** Claude's `TaskOutput` is marked deprecated but still listed.
   Accept it (with optional separate `agent/deprecated-tool-name` warning if/when that
   rule lands).

7. **Capture freshness metadata.** Both pages may change. When the binary-trace
   analysis (`.github/workflows/reverse-binary-analysis.md`) matures, this catalog
   should be replaced. Until then, embed the snapshot date (2026-05-01) and source
   URLs in the lint rule's data file so future maintainers know when to refresh.

---

## Gaps / Limitations

- Neither page provides a machine-readable export of tool names — both are HTML tables
  that need to be parsed/maintained manually.
- copilot-cli's "cloud-agent mapping" column lists names like `view`, `str_replace`,
  `str_replace_editor` whose status as "user-facing tool names valid in `tools:`
  arrays" is ambiguous — they look like internal mappings rather than aliases. Treat
  them as **not** valid for the user-facing lint and confirm via binary-trace analysis
  later.
- Claude's docs note "Your exact tool set depends on your provider, platform, and
  settings." So the 33-tool list is an upper bound; some platforms expose fewer.
- Neither page documents version history of tool-name changes. If a project's `engines`
  specifies a version range, the lint cannot currently validate against version-specific
  tool availability.

---

## Sources

- <https://docs.github.com/en/copilot/reference/custom-agents-configuration#tool-aliases>
- <https://code.claude.com/docs/en/tools-reference>
- <https://code.claude.com/docs/en/tools-reference#bash-tool-behavior>
- <https://code.claude.com/docs/en/tools-reference#lsp-tool-behavior>
- <https://code.claude.com/docs/en/tools-reference#monitor-tool>
- <https://code.claude.com/docs/en/tools-reference#powershell-tool>

## Related Research

- `research/docs/2026-03-28-copilot-cli-migrate-adapter.md` — earlier alias mapping
  discovered via source reading; retained for binary-derived ground truth
- `research/docs/2026-03-31-cli-binary-frontmatter-hook-analysis.md` — Claude Code v2.1.87
  + Copilot CLI v1.0.12 binary analysis (frontmatter, hook events)
- `research/docs/2026-03-28-copilot-cli-source-code-analysis.md` — direct
  source-reading of copilot's `app.js` for plugin/tool surface
- `.github/workflows/reverse-binary-analysis.md` — the workflow that will eventually
  supersede this manual snapshot
