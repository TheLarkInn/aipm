---
date: 2026-03-28 13:30:00 UTC
researcher: Claude
git_commit: b034f7a3c3326ea746e8afd8bee63a7170899ca4
branch: main
repository: aipm
topic: "Copilot CLI v1.0.12 source code analysis — undocumented rules and schemas"
tags: [research, copilot, source-analysis, skills, agents, plugins, mcp, hooks, schemas]
status: complete
last_updated: 2026-03-28
last_updated_by: Claude
---

# Research: Copilot CLI Source Code Analysis (v1.0.12)

## Research Question

What undocumented systems, rules, schemas, and behaviors exist in the Copilot CLI minified JavaScript (`app.js`) that must be accounted for when building the migrate adapter?

## Summary

Analysis of the minified `app.js` (~17MB, 5953 lines), built-in agent YAML definitions, API schemas, and SDK type definitions revealed **20+ undocumented or under-documented behaviors** that diverge from or extend the public GitHub documentation. Key discoveries include: MCP accepts both `"local"` AND `"stdio"` transport types; agent files accept both `.md` and `.agent.md` extensions (not just `.agent.md`); `lspServers` appears in the plugin schema but has zero implementation; 10 hook events exist (not the 6 documented); Copilot reads `CLAUDE.md`, `AGENTS.md`, and `GEMINI.md` as instruction files; and the `${{ secrets }}` interpolation syntax documented in GitHub docs does not exist in the actual code.

---

## Detailed Findings

### 1. Skill Discovery (Source-Verified)

#### Discovery Directories (exact order from `QHt` function)

**Project-level** (with parent-directory inheritance):
1. `<repo-root>/.github/skills/`
2. `<repo-root>/.agents/skills/`
3. `<repo-root>/.claude/skills/`

**Personal/user-level**:
4. `<configDir>/skills/` (where `configDir` = `~/.copilot/` or `$COPILOT_HOME`)
5. `~/.agents/skills/`
6. `~/.claude/skills/`

**Custom**:
7. `COPILOT_SKILLS_DIRS` env var (comma-separated)
8. Paths from `/skills add` command

**Plugin-provided**:
9. Extracted from installed plugins' `plugin.json` `skills` field

Skills are **deduplicated by name, first-found-wins**.

#### Skill Name Validation (Zod schema `vHt`)

- **Regex**: `/^[a-zA-Z0-9][a-zA-Z0-9._\- ]*$/`
- **Max length**: 64 characters
- **Allows**: uppercase letters, dots, underscores, spaces, hyphens (more permissive than documentation suggests)
- **Fallback**: directory name is used when frontmatter `name` is absent

#### Skill Frontmatter Schema (complete, from Zod)

| Field | Type | Required | Constraints |
|-------|------|----------|-------------|
| `name` | string | No | Regex `qCe`, max 64 chars |
| `description` | string | No | Max 1024 chars; falls back to first line of body |
| `allowed-tools` | string or string[] | No | Array joined with `", "` |
| `user-invocable` | boolean | No | Default `true` |
| `disable-model-invocation` | boolean | No | Default `false` |

**Unknown frontmatter fields are silently ignored** (not warned) for skills.

#### Skill Character Budget

- **Default**: 15,000 characters (variable `Mes`)
- **Override**: `SKILL_CHAR_BUDGET` environment variable

---

### 2. Agent Discovery (Source-Verified)

#### Discovery Directories (from `dtn` function)

**GitHub convention**:
- User: `<configDir>/agents/`
- Project: `.github/agents/` (walks up to git root)

**Claude convention**:
- User: `~/.claude/agents/`
- Project: `.claude/agents/` (walks up to git root)

**Plugin agents**: from installed plugin directories

**Remote agents**: fetched from `<copilotUrl>/agents/swe/custom-agents/<owner>/<repo>`

#### Agent File Extension Rules (CRITICAL FINDING)

The source code at function `fvr` reveals:

1. **Both `.md` AND `.agent.md` are accepted** — not just `.agent.md` as documentation implies
2. The filter is simply `c.name.endsWith(".md")`
3. When both `foo.md` and `foo.agent.md` exist, **`.agent.md` takes precedence**
4. Name derivation strips both suffixes: `c.name.replace(/(\.agent)?\.md$/, "")`

#### Agent Frontmatter Schema (complete, from Zod `Flt`)

| Field | Type | Required | Constraints |
|-------|------|----------|-------------|
| `name` | string | No | Display name |
| `description` | string | **Yes** | Required field |
| `tools` | string or string[] | No | Comma-separated string auto-split; default `["*"]` |
| `mcp-servers` | Record<string, McpConfig> | No | Nullable; inline MCP server definitions |
| `infer` | boolean | No | **Legacy** — `!infer` maps to `disableModelInvocation` |
| `disable-model-invocation` | boolean | No | Default `false` |
| `user-invocable` | boolean | No | Default `true` |
| `model` | string | No | Model override |
| `github` | object | No | **Undocumented** — contains `toolsets: string[]` and `permissions: Record<string, string>` |

**Unknown frontmatter fields generate warnings** (via `onUnsupportedFields: "warn"`).

#### Agent Sandboxing Rule

Agents are explicitly told in the system prompt:
> "You cannot access any files in the .github/agents directory. These files contain instructions for other agents."

---

### 3. MCP Server Configuration (Source-Verified)

#### Transport Types (CRITICAL FINDING)

The Zod schema `a5e` accepts **BOTH** `"local"` AND `"stdio"`:

```
type: literal("local") | literal("stdio")  -- optional
```

This means `"stdio"` IS a valid transport type in Copilot CLI, contradicting the documentation which only mentions `"local"`. Type inference logic: if no `type` and no `url`, defaults to `"local"`; if `url` present, defaults to `"http"`.

#### Config File Locations (merge order)

| Priority | Source | Path | `source` field |
|----------|--------|------|---------------|
| 1 (lowest) | User config | `~/.copilot/mcp-config.json` | `"user"` |
| 2 | Workspace `.mcp.json` | `<repo>/.mcp.json` | `"workspace"` |
| 3 | VS Code config | `<repo>/.vscode/mcp.json` | `"workspace"` |
| 4 | Devcontainer | `<repo>/.devcontainer/devcontainer.json` | `"workspace"` |
| 5 | Plugins | `<pluginDir>/mcp-config.json` | `"plugin"` |
| 6 | Windows ODR | Registry discovery | — |
| 7 (highest) | CLI flag | `--additional-mcp-config` | — |
| Built-in | GitHub MCP server | Constructed in code | `"builtin"` |

Last-wins for same server name (opposite of agents/skills which are first-wins).

#### Server Name Validation

Regex: `^[0-9a-zA-Z_.@-]+(/[0-9a-zA-Z_.@-]+)*$`

#### Environment Variable Interpolation (CRITICAL FINDING)

The actual code supports:
- `${VAR_NAME}` — simple reference
- `${VAR_NAME:-default}` — with default value
- `$VAR_NAME` — bare dollar sign

**The `${{ secrets.VAR }}` and `${{ vars.VAR }}` syntax documented in GitHub docs does NOT exist in the code.** This appears to be documentation-only or a future feature.

#### No `headersHelper` Support

Confirmed: `headersHelper` does not exist in the Copilot CLI bundle. This is Claude Code-only.

#### Built-in GitHub MCP Server

- Name: `github-mcp-server`
- URL: `https://api.githubcopilot.com/mcp/readonly` (or `/mcp` when all tools enabled)
- Transport: `"http"`
- `isDefaultServer: true`, `source: "builtin"`
- Specific `filterMapping` for markdown rendering on certain tools

---

### 4. Plugin System (Source-Verified)

#### Plugin Manifest Discovery Order

```javascript
$B = "plugin.json"
e9 = [".plugin", ".", ".github/plugin", ".claude-plugin"]
```

**`.plugin/` is a previously undocumented search location**, appearing first in the search order.

#### Marketplace Manifest Discovery Order

```javascript
mJn = ["marketplace.json", ".plugin/marketplace.json", ".github/plugin/marketplace.json", ".claude-plugin/marketplace.json"]
```

Again, `.plugin/` appears as an additional path.

#### Plugin Name Validation

Regex: `/^[a-zA-Z0-9-]+$/` — max 64 chars. Only letters, numbers, hyphens (no dots, underscores, or spaces unlike skill names).

#### Component Path Schema (`HHt`)

The `skills`, `agents`, and `commands` fields accept three forms:
1. String: `"skills/"` — single path
2. Array: `["skills/", "extra-skills/"]` — multiple paths
3. Object: `{ paths: ["skills/"], exclusive?: boolean }` — when `exclusive: true`, only specified paths are used (no default directory merged)

The `exclusive` field is **undocumented** in GitHub docs.

#### Default Marketplaces

```javascript
z_e = {
  "copilot-plugins": { source: { source: "github", repo: "github/copilot-plugins" } },
  "awesome-copilot": { source: { source: "github", repo: "github/awesome-copilot" } }
}
```

Cannot be removed. Always override user-defined marketplaces with the same name.

#### Cross-Tool Marketplace (`.claude/settings.json`)

Function `deo()` reads `.claude/settings.json` and translates `extraKnownMarketplaces`:
- `{ source: "directory", path }` → `{ source: "local", path }`
- `{ source: "git", url, ref? }` → `{ source: "url", url, ref }`
- `{ source: "github", repo, ref? }` → `{ source: "github", repo, ref }`

#### Template Variable Substitution

Plugin manifest fields support `${PLUGIN_ROOT}` and `${CLAUDE_PLUGIN_ROOT}` — both resolve to the plugin install directory.

#### `strict` Field Behavior

- Marketplace plugins: `strict: true` (default)
- Direct repo installs: `strict: false` (set by `xHt()`)
- Effect: controls validation strictness during loading

#### `lspServers` — Schema Only, No Implementation (CRITICAL FINDING)

The `lspServers` field exists in the plugin manifest Zod schema (`qes`), but **there is zero implementation in the bundle**. Searching for `lspServers` returns no matches beyond the schema definition. LSP server configuration is not functional in Copilot CLI v1.0.12.

#### `outputStyles` Field

Present in plugin schema: `re.union([re.string(), re.array(re.string())]).optional()`. This is undocumented in GitHub docs but functional.

---

### 5. Hooks System (Source-Verified)

#### Hook Event Names (10 events, with legacy mapping)

**Canonical events** (from `b4n` Set):

| Canonical Name | Legacy Name(s) |
|---------------|----------------|
| `sessionStart` | `SessionStart` |
| `sessionEnd` | `SessionEnd` |
| `userPromptSubmitted` | `UserPromptSubmit` |
| `preToolUse` | `PreToolUse` |
| `postToolUse` | `PostToolUse` |
| `errorOccurred` | `PostToolUseFailure`, `ErrorOccurred` |
| `agentStop` | `Stop` |
| `subagentStop` | `SubagentStop` |
| `subagentStart` | (no legacy) |
| `preCompact` | `PreCompact` |

The legacy-to-canonical mapping function `BFe()` normalizes keys at load time and merges arrays when both old and new keys exist.

#### Hook Configuration Files

- `hooks.json` at plugin root
- `hooks/hooks.json` in plugin directory
- `**/*.json` within the hooks directory (globbed)
- Inline in `plugin.json` under `hooks` key

#### Hook Command Schema (Zod `AFt`)

```
type: literal("command") (default)
bash: string (optional)
powershell: string (optional)
command: string (optional) — copied to bash/powershell if those are absent
timeout: number, positive (optional) — becomes timeoutSec
```

**At least one of `bash`, `powershell`, or `command` must be specified.**

#### Special Hook Features

- `subagentStart` and `preCompact` hooks support a `matcher` field (regex pattern)
- `sessionStart` uniquely accepts prompt-type hooks: `{ type: "prompt", prompt: string }`
- Hook execution is **sequential** (not parallel) within an event
- Non-zero exit with stdout is treated as a warning (not an error)
- Return values are merged across all hooks for an event

#### Hook Event Data and Return Values

| Event | Key Return Fields |
|-------|-----------------|
| `sessionStart` | `additionalContext` (prepended to conversation) |
| `userPromptSubmitted` | `modifiedPrompt` (replaces user input) |
| `preToolUse` | `permissionDecision` ("deny"/"ask"), `modifiedArgs` |
| `postToolUse` | `modifiedResult` |
| `agentStop` | `decision` ("block"), `reason` (re-queued as user message) |
| `subagentStart` | `additionalContext` |
| `subagentStop` | `decision`, `reason` |

---

### 6. Instruction File Discovery (Undocumented Cross-Tool Feature)

Copilot CLI discovers instruction files from multiple conventions:

| Kind | Convention Dir | Filename |
|------|---------------|----------|
| `copilot` | `.github` | `copilot-instructions.md` |
| `agents` | `.` (root) | `AGENTS.md` |
| `claude` | `.` (root) | `CLAUDE.md` |
| `claude` | `.claude` | `CLAUDE.md` |
| `gemini` | `.` (root) | `GEMINI.md` |

Additionally, **scoped instructions** from `.github/instructions/*.instructions.md` (globbed recursively) support:
- `applyTo` frontmatter field — glob pattern for file-scoped targeting
- `excludeAgent` frontmatter field — agent names to exclude

---

### 7. Built-in Agent Definitions (from `definitions/` directory)

Five built-in agents ship as `.agent.yaml` files with a richer schema than custom agents:

| Agent | Model | Side Effects | Contexts |
|-------|-------|-------------|----------|
| `explore` | `claude-haiku-4.5` | No | — |
| `task` | `claude-haiku-4.5` | Yes | — |
| `code-review` | `claude-sonnet-4.5` | No | — |
| `configure-copilot` | `claude-haiku-4.5` | Yes | `["cli"]` |
| `research` | `claude-sonnet-4.6` | — | loaded on-demand |

Built-in agent schema includes fields unavailable to custom agents:
- `displayName` — separate from `name`
- `promptParts` — granular system prompt section control
- `contexts` — `"cli"`, `"cca"`, `"sdk"` (visibility scoping)
- `featureFlag` — gates availability behind experiment flags

#### `promptParts` Options

| Flag | Default | Effect |
|------|---------|--------|
| `includeAISafety` | `true` | Include safety guardrails |
| `includeToolInstructions` | `true` | Include per-tool usage instructions |
| `includeParallelToolCalling` | `false` | Include parallel tool calling guidance |
| `includeCustomAgentInstructions` | `false` | Include user custom instructions |
| `includeEnvironmentContext` | `true` | Include cwd, OS, tools context |

#### Template Variables in Built-in Agents

| Variable | Resolution |
|----------|-----------|
| `{{cwd}}` | `process.cwd()` |
| `{{homedir}}` | `os.homedir()` |
| `{{configDir}}` | Copilot config directory |
| `{{grepToolName}}` | `grep` or `rg` |
| `{{globToolName}}` | `glob` |
| `{{shellToolName}}` | `bash` or `powershell` |
| `{{shellCommandExamples}}` | Platform-dependent examples |
| `{{mcpSchema}}` | Generated from MCP Zod schemas |
| `{{serverNamePattern}}` | MCP server name regex |
| `{{reportPath}}` | Research output path |

---

### 8. SDK API Surface (from schemas)

#### Key Experimental RPC Methods

| Method | Purpose |
|--------|---------|
| `session.agent.list` | List custom agents |
| `session.agent.select/deselect/reload` | Agent management |
| `session.skills.list/enable/disable/reload` | Skill management |
| `session.mcp.list/enable/disable/reload` | MCP server management |
| `session.plugins.list` | List installed plugins (**read-only**) |
| `session.extensions.list/enable/disable/reload` | Extension management |
| `session.compaction.compact` | Trigger context compaction |

#### Extensions System (Undocumented)

Extensions are a separate concept from agents/skills/plugins:
- Discovered from `.github/extensions/` (project) and `~/.copilot/extensions/` (user)
- Run as child processes with their own PID
- Statuses: `"running"`, `"disabled"`, `"failed"`, `"starting"`
- Managed via `session.extensions.*` RPC methods

#### System Prompt Sections

10 customizable sections:
`identity`, `tone`, `tool_efficiency`, `environment_context`, `code_change_rules`, `guidelines`, `safety`, `tool_instructions`, `custom_instructions`, `last_instructions`

---

## Corrections to Public Documentation

| Documentation Claim | Source Code Reality |
|---------------------|-------------------|
| Agent files must use `.agent.md` extension | Both `.md` and `.agent.md` work; `.agent.md` takes precedence |
| MCP `type` must be `"local"` for stdio | Both `"local"` and `"stdio"` are accepted |
| `${{ secrets.VAR }}` interpolation supported | Not implemented; only `${VAR}`, `${VAR:-default}`, `$VAR` |
| `lspServers` in plugin.json configures LSP | Schema-only; zero implementation |
| Plugin manifest at `.github/plugin/plugin.json` | Also searched at `.plugin/plugin.json` (first in search order) |
| Docs mention 6 hook events | 10 hook events exist including `subagentStart`, `subagentStop`, `preCompact`, `errorOccurred` |
| Skill name: "lowercase with hyphens" | Regex allows uppercase, dots, underscores, spaces |
| No mention of instruction file cross-reading | Copilot reads `CLAUDE.md`, `AGENTS.md`, `GEMINI.md` |

---

## Implications for Migrate Adapter

### What Changes from Previous Research

1. **MCP transport**: No normalization needed — Copilot accepts both `"local"` and `"stdio"`. The adapter can pass through either value.

2. **Agent file extension**: The adapter must handle both `.md` and `.agent.md` files, with `.agent.md` taking precedence in dedup.

3. **LSP**: Do NOT implement an LSP detector — the feature is schema-only with no runtime support.

4. **Skill schema**: The name regex is more permissive than documented (`[a-zA-Z0-9._\- ]`). The adapter should respect the actual Zod schema, not the documentation.

5. **Plugin search path**: Include `.plugin/` as a plugin manifest location in addition to `.github/plugin/` and `.claude-plugin/`.

6. **Hook events**: Use the canonical names (`sessionStart`, not `SessionStart`), but support legacy name mapping for compatibility.

7. **Instruction files**: The migrate adapter should be aware that Copilot reads `CLAUDE.md` files — migration doesn't need to handle these, but lint rules might want to check for conflicts.

8. **Extensions**: A new component type exists (`.github/extensions/`) that is separate from agents/skills/plugins. Consider whether the adapter should detect these.

### New Detector Requirements

Based on source code analysis, the Copilot adapter needs:

| Detector | Source Path | File Pattern | Notes |
|----------|------------|-------------|-------|
| `CopilotSkillDetector` | `.github/skills/` | `<dir>/SKILL.md` | Same as Claude; can likely reuse |
| `CopilotAgentDetector` | `.github/agents/` | `*.md` and `*.agent.md` | `.agent.md` precedence; extra fields |
| `CopilotMcpDetector` | `.copilot/mcp-config.json`, `.mcp.json` | JSON | Both `"local"` and `"stdio"` valid |
| `CopilotHookDetector` | `hooks.json`, `hooks/hooks.json` | JSON | Standalone files, not extracted from settings |
| ~~CopilotLspDetector~~ | — | — | **DO NOT IMPLEMENT** — no runtime support |

---

## Code References

- `app.js` install path: `/home/codespace/.agency/nodejs/node-v22.21.0-linux-x64/lib/node_modules/@github/copilot/`
- `app.js` size: 16,956,142 bytes, 5,953 lines
- Built-in agents: `definitions/*.agent.yaml` (5 files)
- API schema: `schemas/api.schema.json` (2,183 lines, all JSON-RPC methods)
- Session events: `schemas/session-events.schema.json` (7,779 lines, 59 event types)
- SDK types: `copilot-sdk/types.d.ts` (1,028 lines)

---

## Related Research

- [`research/docs/2026-03-28-copilot-cli-migrate-adapter.md`](research/docs/2026-03-28-copilot-cli-migrate-adapter.md) — Documentation-based adapter research (this document corrects/extends it)
- [`research/tickets/2026-03-28-110-aipm-lint.md`](research/tickets/2026-03-28-110-aipm-lint.md) — Lint command research
- [`research/docs/2026-03-16-copilot-agent-discovery.md`](research/docs/2026-03-16-copilot-agent-discovery.md) — Earlier Copilot discovery research

---

## Open Questions (Partially Resolved 2026-03-28)

1. **Extensions system**: **RESOLVED** — Include in scope. Add `ArtifactKind::Extension` and `CopilotExtensionDetector`. Extensions run as child processes but should still be migrated.

2. **`outputStyles` in plugin schema**: Present but undocumented. Is this a passthrough from Claude Code compatibility, or does Copilot actually use output styles? *(Unresolved — not blocking for migrate adapter.)*

3. **`exclusive` component paths**: The `{ paths, exclusive }` form for skills/agents/commands is undocumented. Should the adapter preserve this during migration? *(Unresolved — not blocking; raw_content passthrough will preserve it.)*

4. **Skill character budget**: The 15,000 char default (`SKILL_CHAR_BUDGET`) is different from the 5,000 token limit in the BDD feature spec. Which should lint rules enforce? *(Unresolved — deferred to lint spec.)*

5. **`github` frontmatter field on agents**: Contains `toolsets` and `permissions`. Is this used for GitHub Actions integration only, or should it be preserved during migration? *(Unresolved — raw_content passthrough will preserve it regardless.)*
