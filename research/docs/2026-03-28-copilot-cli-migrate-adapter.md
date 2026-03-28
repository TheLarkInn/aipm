---
date: 2026-03-28 12:55:06 UTC
researcher: Claude
git_commit: b034f7a3c3326ea746e8afd8bee63a7170899ca4
branch: main
repository: aipm
topic: "Copilot CLI migrate adapter — format mapping and detector design"
tags: [research, codebase, copilot, migrate, adapter, detector, skills, agents, mcp, plugins]
status: complete
last_updated: 2026-03-28
last_updated_by: Claude
---

# Research: Copilot CLI Migrate Adapter

## Research Question

To prepare for `aipm lint`, we need to build the adapter for `aipm migrate` for Copilot CLI. What are the Copilot CLI customization formats (skills, plugins, agents, MCP servers), how do they map to the existing `Artifact`/`ArtifactKind` system, and what detectors need to be implemented?

## Summary

Copilot CLI and Claude Code share the **Agent Skills specification** (agentskills.io) — skills use identical `SKILL.md` files with YAML frontmatter in both tools. Agents differ: Copilot uses `.agent.md` files (with `description` required, `name` optional) vs Claude's plain `.md` files. MCP config uses `"local"` instead of `"stdio"` for transport type. The existing `Detector` trait, `Artifact` types, and factory-based dispatch pattern can be directly reused — a `copilot_detectors()` factory function with 5 detectors (skill, agent, MCP, hook, LSP) would mirror the existing `claude_detectors()`.

---

## Detailed Findings

### 1. Copilot CLI Directory Layout

Copilot CLI uses `.github/` as its primary project-level configuration directory, with some files in `.copilot/`.

#### Project-Level Directories

| Component | Claude Code | Copilot CLI |
|-----------|------------|-------------|
| Skills | `.claude/skills/<name>/SKILL.md` | `.github/skills/<name>/SKILL.md` |
| Agents | `.claude/agents/<name>.md` | `.github/agents/<name>.agent.md` |
| MCP config | `.mcp.json` (project root) | `.copilot/mcp-config.json` or `.mcp.json` |
| Hooks | `.claude/settings.json` `hooks` key | `hooks.json` or `hooks/hooks.json` |
| LSP config | (not in `.claude/`) | `lsp.json` or `.github/lsp.json` |
| Commands | `.claude/commands/<name>.md` | (same concept, within plugins) |
| Output Styles | `.claude/output-styles/<name>.md` | (no Copilot equivalent) |
| Plugin manifest | `.claude-plugin/plugin.json` | `.github/plugin/plugin.json` |
| Marketplace | `.claude-plugin/marketplace.json` | `.github/plugin/marketplace.json` |

#### User-Level Directories

| Component | Claude Code | Copilot CLI |
|-----------|------------|-------------|
| Skills | `~/.claude/skills/` | `~/.copilot/skills/`, `~/.agents/skills/` |
| Agents | `~/.claude/agents/` | `~/.copilot/agents/` |
| MCP config | `~/.claude.json` | `~/.copilot/mcp-config.json` |

**Source**: [GitHub Docs — Creating agent skills](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/create-skills), [GitHub Docs — Creating plugins](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/plugins-creating)

---

### 2. Copilot CLI Skills Format

Skills follow the **Agent Skills specification** (https://agentskills.io/specification) — the same open standard as Claude Code.

**File**: `SKILL.md` inside a named subdirectory (identical to Claude Code)

**Directory layout**:
```
.github/skills/
└── my-skill/
    ├── SKILL.md          # Required
    ├── scripts/          # Optional
    ├── references/       # Optional
    └── assets/           # Optional
```

**YAML Frontmatter Fields**:

| Field | Required | Constraints |
|-------|----------|-------------|
| `name` | **Yes** | Max 64 chars. Lowercase + hyphens. Must match parent directory name. |
| `description` | **Yes** | Max 1024 chars. Non-empty. |
| `license` | No | License name or reference |
| `compatibility` | No | Max 500 chars. Environment requirements. |
| `metadata` | No | Arbitrary key-value mapping |
| `allowed-tools` | No | **Experimental.** Space-delimited tool names. |

**Key Insight**: The `name` and `description` fields are required by the Agent Skills spec (both for Claude and Copilot), but the existing `SkillDetector` at [`skill_detector.rs:44`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/skill_detector.rs#L44) falls back to the directory name when `name` is absent. For Copilot skills, the spec mandates `name` must match the parent directory.

**Script references** use the same `${CLAUDE_SKILL_DIR}/scripts/` variable syntax (per the agent skills spec, this is a generic `${SKILL_DIR}` convention; the existing `extract_script_references()` at [`skill_detector.rs:134-155`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/skill_detector.rs#L134-L155) searches for `${CLAUDE_SKILL_DIR}/`).

**Source**: [Agent Skills Specification](https://agentskills.io/specification), [GitHub Docs — Creating agent skills](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/create-skills)

---

### 3. Copilot CLI Agents Format

Agents use a **different file format** than Claude Code.

**File**: `<name>.agent.md` (not `<name>.md`)
**Directory**: `.github/agents/`

**YAML Frontmatter Fields**:

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `description` | **Yes** | — | Agent's purpose and capabilities |
| `name` | No | (none) | Display name |
| `model` | No | inherited | Model to use |
| `tools` | No | all tools | List or `["*"]` or `[]` |
| `target` | No | `both` | `vscode`, `github-copilot`, or `both` |
| `disable-model-invocation` | No | `false` | Prevent auto-delegation |
| `user-invocable` | No | `true` | Can user invoke directly |
| `mcp-servers` | No | (none) | Inline MCP server config |
| `metadata` | No | (none) | Arbitrary key-value data |

**Key Differences from Claude Code agents**:
1. **File extension**: `.agent.md` vs `.md`
2. **Required field**: `description` is required (not `name`). In Claude Code, the `AgentDetector` at [`agent_detector.rs:64-95`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/agent_detector.rs#L64-L95) parses `name` and `description` but neither is enforced as required.
3. **Extra fields**: `tools`, `model`, `target`, `disable-model-invocation`, `user-invocable`, `mcp-servers` — the Claude `AgentDetector` currently only extracts `name` and `description`.
4. **Max body size**: 30,000 characters (documented for Copilot, no equivalent limit documented for Claude).
5. **Inline MCP**: Agents can embed MCP server definitions directly in frontmatter:
   ```yaml
   mcp-servers:
     server-name:
       type: 'local'
       command: 'command-name'
       args: ['--arg1']
       tools: ["*"]
       env:
         VAR: ${{ secrets.VAR }}
   ```

**Tool aliases**: Copilot CLI maps Claude Code tool names to its own equivalents (case insensitive):

| Copilot Name | Claude Code Aliases |
|-------------|-------------------|
| `execute` | `Bash`, `shell`, `powershell` |
| `read` | `Read`, `NotebookRead` |
| `edit` | `Edit`, `MultiEdit`, `Write`, `NotebookEdit` |
| `search` | `Grep`, `Glob` |
| `agent` | `Task`, `custom-agent` |
| `web` | `WebSearch`, `WebFetch` |
| `todo` | `TodoWrite` |

**Source**: [GitHub Docs — Custom agents](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/create-custom-agents-for-cli), [Custom agents configuration reference](https://docs.github.com/en/copilot/reference/custom-agents-configuration)

---

### 4. Copilot CLI MCP Server Configuration

**File**: `~/.copilot/mcp-config.json` (user-level) or `.mcp.json` / `.github/mcp.json` (project-level, via plugins)
**Format**: JSON with `mcpServers` top-level key (same key as Claude Code)

**Transport Types**:

| Copilot CLI | Claude Code | Description |
|------------|------------|-------------|
| `"local"` | `"stdio"` | Local process over stdin/stdout |
| `"http"` | `"http"` | Streamable HTTP |
| `"sse"` | `"sse"` | Server-Sent Events (deprecated) |

**Key Difference**: Copilot uses `"local"` where Claude Code and the MCP spec use `"stdio"`. A Copilot adapter would need to normalize this during migration.

**Local server fields**:

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | `"local"` |
| `command` | string | Executable to run |
| `args` | array | Command-line arguments |
| `env` | object | Environment variables (`PATH` auto-inherited) |
| `tools` | string/array | `"*"` or comma-separated tool names |

**HTTP/SSE server fields**:

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | `"http"` or `"sse"` |
| `url` | string | Remote server URL |
| `headers` | object | HTTP headers (for auth) |
| `tools` | string/array | `"*"` or comma-separated tool names |

**Copilot config example**:
```json
{
  "mcpServers": {
    "playwright": {
      "type": "local",
      "command": "npx",
      "args": ["@playwright/mcp@latest"],
      "env": {},
      "tools": ["*"]
    }
  }
}
```

**What's missing in Copilot CLI vs Claude Code**: No OAuth support, no `headersHelper`, no env var interpolation (`${VAR}` syntax), no managed enterprise config, no project-scope approval prompts.

**Built-in**: GitHub MCP server is built into Copilot CLI (no config needed).

**Source**: [GitHub Docs — Add MCP servers](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers)

---

### 5. Copilot CLI Plugin Format

Plugins are the **distribution/packaging layer** on top of skills, agents, hooks, MCP, and LSP.

**Manifest file**: `plugin.json` (at plugin root, `.github/plugin/plugin.json`, or `.claude-plugin/plugin.json`)

**Required field**: `name` (kebab-case, max 64 chars)

**Optional fields**: `description`, `version`, `author` (object with `name`/`email`/`url`), `homepage`, `repository`, `license`, `keywords`, `category`, `tags`

**Component path fields**:

| Field | Type | Default | Maps to |
|-------|------|---------|---------|
| `agents` | string/string[] | `agents/` | Agent `.agent.md` files |
| `skills` | string/string[] | `skills/` | Skill directories with `SKILL.md` |
| `commands` | string/string[] | — | Command directories |
| `hooks` | string/object | — | `hooks.json` or inline |
| `mcpServers` | string/object | — | `.mcp.json` or inline |
| `lspServers` | string/object | — | `lsp.json` or inline |

**Marketplace manifest** (`marketplace.json`): Same format as Claude Code's `marketplace.json` — `name`, `owner`, `metadata`, `plugins[]` array.

**Key Insight**: Copilot CLI explicitly recognizes `.claude-plugin/` as an alternative to `.github/plugin/`. The formats are intentionally compatible. Default marketplaces include both `github/copilot-plugins` and `anthropics/claude-code`.

**Source**: [GitHub Docs — Creating plugins](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/plugins-creating), [Plugin reference](https://docs.github.com/en/copilot/reference/cli-plugin-reference)

---

### 6. Mapping Copilot Components to Existing ArtifactKind

The current `ArtifactKind` enum at [`mod.rs:22-35`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/mod.rs#L22-L35) has six variants. Here's how Copilot components map:

| Copilot Component | Existing ArtifactKind | Notes |
|-------------------|----------------------|-------|
| `.github/skills/<name>/SKILL.md` | `ArtifactKind::Skill` | **Exact same format** (Agent Skills spec) |
| `.github/agents/<name>.agent.md` | `ArtifactKind::Agent` | Different extension (`.agent.md` vs `.md`), different required fields |
| `.copilot/mcp-config.json` or `.mcp.json` | `ArtifactKind::McpServer` | `"local"` → `"stdio"` normalization needed |
| `hooks.json` or `hooks/hooks.json` | `ArtifactKind::Hook` | Same JSON format as Claude hooks |
| `lsp.json` or `.github/lsp.json` | (new: `ArtifactKind::LspServer`) | No existing detector; new kind needed |
| `.github/agents/*.agent.md` `commands` | `ArtifactKind::Command` | If Copilot has command files |

**No Copilot equivalent for**: `ArtifactKind::OutputStyle` (Claude-specific)

---

### 7. Existing Codebase Extension Points

#### Source-Type Dispatch
[`crates/libaipm/src/migrate/mod.rs:264-267`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/mod.rs#L264-L267)

```rust
let detectors = match source {
    ".claude" => detector::claude_detectors(),
    other => return Err(Error::UnsupportedSource(other.to_string())),
};
```

A Copilot adapter adds: `".github" => detector::copilot_detectors()`

#### Detector Factory
[`crates/libaipm/src/migrate/detector.rs:22-32`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/detector.rs#L22-L32)

New factory function needed:
```rust
pub fn copilot_detectors() -> Vec<Box<dyn Detector>> {
    vec![
        Box::new(super::copilot_skill_detector::CopilotSkillDetector),
        Box::new(super::copilot_agent_detector::CopilotAgentDetector),
        Box::new(super::copilot_mcp_detector::CopilotMcpDetector),
        Box::new(super::copilot_hook_detector::CopilotHookDetector),
        Box::new(super::copilot_lsp_detector::CopilotLspDetector),
    ]
}
```

#### Error Message
[`crates/libaipm/src/migrate/mod.rs:172-174`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/mod.rs#L172-L174)

Currently: `"unsupported source type '{0}' — supported sources: .claude"` — needs updating.

#### Module Registration
[`crates/libaipm/src/migrate/mod.rs:1-13`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/mod.rs#L1-L13)

New `pub mod` declarations needed for each Copilot detector.

#### Emitter Dispatch
[`crates/libaipm/src/migrate/emitter.rs:63-84`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/emitter.rs#L63-L84) (3 match sites + plugin.json generation)

If new `ArtifactKind` variants are added (e.g., `LspServer`), new match arms needed in all four dispatch sites.

#### Discovery
[`crates/libaipm/src/migrate/discovery.rs`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/discovery.rs)

Currently `discover_claude_dirs()` scans for `.claude/` directories. A parallel `discover_github_dirs()` or parameterized `discover_source_dirs(pattern)` would scan for `.github/` directories.

#### Workspace Init Adaptor
[`crates/libaipm/src/workspace_init/adaptors/mod.rs:7-15`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/workspace_init/adaptors/mod.rs#L7-L15)

Comment says: "Future adaptors (Copilot CLI, OpenCode, etc.) are added here." A `copilot.rs` implementing `ToolAdaptor` would be registered in `defaults()`.

---

### 8. Copilot Detector Implementation Details

#### CopilotSkillDetector

**Scan directory**: `<source_dir>/skills/` (where `source_dir` = `.github/`)
**File pattern**: subdirectories containing `SKILL.md`
**Frontmatter**: Same as Claude — `name`, `description`, `license`, `compatibility`, `metadata`, `allowed-tools`
**Script references**: Same `${CLAUDE_SKILL_DIR}/scripts/` pattern (or possibly a tool-agnostic `${SKILL_DIR}/` — needs verification)
**Output**: `ArtifactKind::Skill`

**Implementation**: Could potentially **reuse `SkillDetector` directly** since the format is identical. The only question is whether the script reference variable name differs.

#### CopilotAgentDetector

**Scan directory**: `<source_dir>/agents/`
**File pattern**: `*.agent.md` files (NOT `*.md` — this is a key difference)
**Frontmatter parsing**: Must extract `description` (required), `name`, `model`, `tools`, `target`, `disable-model-invocation`, `user-invocable`, `mcp-servers`, `metadata`
**Name derivation**: Strip `.agent.md` suffix from filename (e.g., `security-auditor.agent.md` → `security-auditor`)
**Output**: `ArtifactKind::Agent`

**Additional metadata fields needed**: The current `ArtifactMetadata` at [`mod.rs:53-65`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/mod.rs#L53-L65) has `name`, `description`, `hooks`, `model_invocation_disabled`, `raw_content`. The Copilot agent format adds `tools`, `model`, `target`, `user_invocable`, and inline `mcp_servers` — these would either need new fields on `ArtifactMetadata` or be stored in `raw_content`.

#### CopilotMcpDetector

**Scan files**: `<project_root>/.copilot/mcp-config.json` or `<project_root>/.mcp.json` or `<project_root>/.github/mcp.json`
**JSON key**: `mcpServers` (same as Claude)
**Transport normalization**: `"local"` → `"stdio"` when migrating to `.ai/` format
**Output**: `ArtifactKind::McpServer`

**Note**: The existing `McpDetector` at [`mcp_detector.rs:12-61`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/mcp_detector.rs#L12-L61) reads `<project_root>/.mcp.json`. A Copilot version checks multiple locations and normalizes transport type names.

#### CopilotHookDetector

**Scan files**: `<source_dir>/hooks.json` or `<source_dir>/hooks/hooks.json`
**Format**: JSON with hooks array
**Output**: `ArtifactKind::Hook`

**Note**: The existing `HookDetector` reads `.claude/settings.json` and extracts the `hooks` key. A Copilot version reads standalone `hooks.json` files directly — which is simpler.

#### CopilotLspDetector

**Scan files**: `<source_dir>/lsp.json` or `<source_dir>/../.github/lsp.json`
**Format**: JSON with LSP server definitions
**Output**: New `ArtifactKind::LspServer` (or reuse `ArtifactKind::McpServer` since both are server configs)

**Note**: No existing detector handles LSP. This is a new artifact kind.

---

### 9. Copilot Loading Precedence (Context for Lint Rules)

Copilot CLI has a defined loading order that determines which definitions win when duplicates exist:

**Agents/Skills (first-found-wins)**:
1. `~/.copilot/agents/` (personal)
2. `.github/agents/` (project)
3. Parent `.github/agents/` (monorepo inheritance)
4. `~/.claude/agents/` (Claude personal)
5. `.claude/agents/` (Claude project)
6. Plugin agents (by install order)
7. Remote org/enterprise agents

**MCP Servers (last-found-wins)**:
1. `~/.copilot/mcp-config.json` (lowest priority)
2. `.vscode/mcp.json`
3. Plugin MCP configs
4. `--additional-mcp-config` flag (highest)

This is relevant for lint rules that check for naming conflicts across sources.

**Source**: [GitHub Docs — Creating plugins](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/plugins-creating)

---

## Code References

### Existing Detector Pattern
- [`crates/libaipm/src/migrate/detector.rs:13-20`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/detector.rs#L13-L20) — `Detector` trait
- [`crates/libaipm/src/migrate/detector.rs:22-32`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/detector.rs#L22-L32) — `claude_detectors()` factory
- [`crates/libaipm/src/migrate/mod.rs:264-267`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/mod.rs#L264-L267) — Source-type dispatch
- [`crates/libaipm/src/migrate/mod.rs:22-96`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/mod.rs#L22-L96) — `ArtifactKind`, `ArtifactMetadata`, `Artifact`

### Existing Detectors (Reference Implementations)
- [`crates/libaipm/src/migrate/skill_detector.rs`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/skill_detector.rs) — Most complex: directory-based, frontmatter, scripts, recursive file collection
- [`crates/libaipm/src/migrate/agent_detector.rs`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/agent_detector.rs) — Flat `.md` files
- [`crates/libaipm/src/migrate/mcp_detector.rs`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/mcp_detector.rs) — JSON config at project root
- [`crates/libaipm/src/migrate/hook_detector.rs`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/hook_detector.rs) — JSON extraction from settings.json

### Extension Points
- [`crates/libaipm/src/migrate/mod.rs:172-174`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/mod.rs#L172-L174) — `UnsupportedSource` error message
- [`crates/libaipm/src/migrate/emitter.rs:63-84`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/migrate/emitter.rs#L63-L84) — Emitter kind dispatch (3 sites + plugin.json)
- [`crates/libaipm/src/workspace_init/adaptors/mod.rs:7-15`](https://github.com/TheLarkInn/aipm/blob/b034f7a3c3326ea746e8afd8bee63a7170899ca4/crates/libaipm/src/workspace_init/adaptors/mod.rs#L7-L15) — ToolAdaptor registry

---

## Architecture Documentation

### Format Compatibility Matrix

| Feature | Claude Code | Copilot CLI | Compatible? |
|---------|------------|-------------|-------------|
| Skill file (`SKILL.md`) | Yes | Yes | **Identical** (Agent Skills spec) |
| Skill frontmatter | `name`, `description`, `hooks`, `disable-model-invocation` | `name`, `description`, `license`, `compatibility`, `metadata`, `allowed-tools` | Superset (Copilot has more fields) |
| Agent file format | `<name>.md` | `<name>.agent.md` | **Different extension** |
| Agent required fields | none enforced | `description` required | Different |
| Agent extra fields | — | `tools`, `model`, `target`, `user-invocable`, `disable-model-invocation`, `mcp-servers` | Copilot richer |
| MCP top-level key | `mcpServers` | `mcpServers` | **Identical** |
| MCP stdio type | `"stdio"` | `"local"` | **Different value** |
| MCP tools filter | (not in config) | `tools` field | Copilot-only |
| MCP env var syntax | `${VAR}`, `${VAR:-default}` | `$VAR`, `${VAR}`, `${{ secrets.VAR }}` | Partially compatible |
| Hooks format | JSON in `settings.json` | standalone `hooks.json` | Different source, same target format |
| Plugin manifest | `.claude-plugin/plugin.json` | `.github/plugin/plugin.json` | Same schema, different path |
| Marketplace manifest | `.claude-plugin/marketplace.json` | `.github/plugin/marketplace.json` | Same schema, different path |
| Output Styles | Yes | No | Claude-only |
| LSP config | No | Yes | Copilot-only |
| Commands | `.claude/commands/` | (within plugins) | Similar |

### Design Consideration: Reuse vs. New Detectors

Given the high format compatibility, there are two approaches:

**Approach A — Separate Copilot Detectors**: Create `copilot_skill_detector.rs`, `copilot_agent_detector.rs`, etc. Each is a thin struct implementing `Detector`. This follows the existing pattern exactly and allows Copilot-specific logic (`.agent.md` extension, `"local"` transport type) without complicating existing detectors.

**Approach B — Parameterized Detectors**: Make existing detectors configurable (e.g., agent file extension, MCP transport type name). Risk: bloats existing clean implementations with conditional logic.

The existing codebase favors Approach A — each detector is a focused, single-purpose unit struct. The `claude_detectors()` and `copilot_detectors()` factories select the appropriate set.

### Skill Detector Reuse Potential

Since skills use the **identical format** (Agent Skills spec), the existing `SkillDetector` could be reused directly for Copilot skills. The only question: does Copilot use `${CLAUDE_SKILL_DIR}/` or a different variable name for script references? The Agent Skills spec doesn't mandate a specific variable name. The existing `extract_script_references()` searches specifically for `${CLAUDE_SKILL_DIR}/`.

---

## Historical Context (from research/)

- `research/docs/2026-03-16-copilot-agent-discovery.md` — Maps Copilot CLI's discovery directories and configuration files. Confirms `.github/agents/` for agents, `~/.copilot/` for user-level config, `.copilot/mcp-config.json` for MCP servers.

- `research/docs/2026-03-10-microsoft-apm-analysis.md` — Documents AIPM's architectural decision to decouple package management from tool-specific context generation. A Copilot adapter should configure discovery paths, NOT compile/transform content.

- `research/docs/2026-03-19-init-tool-adaptor-refactor.md` — Documents the `ToolAdaptor` trait refactor. The old code had a `write_copilot_config()` function that was deleted. A new `copilot.rs` adaptor needs to be created following the `claude.rs` pattern.

- `research/tickets/2026-03-28-110-aipm-lint.md` — The lint command research. Documents how the detector pattern maps to lint rules and identifies that the same adapter architecture should be shared.

---

## Related Research

- [`research/tickets/2026-03-28-110-aipm-lint.md`](research/tickets/2026-03-28-110-aipm-lint.md) — `aipm lint` research (same adapter architecture)
- [`research/docs/2026-03-16-copilot-agent-discovery.md`](research/docs/2026-03-16-copilot-agent-discovery.md) — Earlier Copilot discovery research
- [`research/docs/2026-03-10-microsoft-apm-analysis.md`](research/docs/2026-03-10-microsoft-apm-analysis.md) — microsoft/apm competitive analysis
- [`research/docs/2026-03-19-init-tool-adaptor-refactor.md`](research/docs/2026-03-19-init-tool-adaptor-refactor.md) — ToolAdaptor trait pattern
- [`research/docs/2026-03-23-aipm-migrate-command.md`](research/docs/2026-03-23-aipm-migrate-command.md) — Original migrate command research
- [`research/docs/2026-03-24-migrate-all-artifact-types.md`](research/docs/2026-03-24-migrate-all-artifact-types.md) — All artifact type detectors

---

## Open Questions (Resolved 2026-03-28)

All open questions have been resolved via design review. Decisions are recorded below.

1. **Skill script variable name**: Still unknown whether Copilot uses `${CLAUDE_SKILL_DIR}/` or `${SKILL_DIR}/`. The Agent Skills spec doesn't prescribe a name. **Decision**: Create a thin `CopilotSkillDetector` wrapper that delegates to shared extraction logic, allowing the script variable name to differ without touching the original `SkillDetector`.

2. **Should `SkillDetector` be reused directly?** **Decision**: Thin wrapper with shared logic (option a). Extract common skill parsing into shared functions; `CopilotSkillDetector` and `SkillDetector` both call them but can diverge on tool-specific details like script variable names.

3. **`ArtifactMetadata` expansion**: **Decision**: `raw_content` passthrough. Store the full file content in `raw_content` and let the emitter pass it through unchanged. No new fields on `ArtifactMetadata`. Keeps the struct tool-agnostic.

4. **LSP as new ArtifactKind?** **Decision**: Yes — implement `ArtifactKind::LspServer` and a `CopilotLspDetector` even though Copilot v1.0.12 has no runtime support. Future-proofing was preferred over minimalism.

5. **MCP transport normalization**: **Decision**: Pass through as-is. Source code confirms Copilot accepts both `"local"` and `"stdio"`. No normalization needed during migration.

6. **Recursive discovery**: **Decision**: Parameterize into a generic `discover_source_dirs(patterns)` function. Both `.claude/` and `.github/` discovered in one walk. The existing `discover_claude_dirs()` becomes a thin wrapper or is replaced.

7. **Source argument value**: **Decision**: `--source` is just a path location. The Copilot detectors should be automatically registered alongside Claude detectors regardless of the `--source` value. The dispatch logic should not gate on directory name — instead, all registered detector sets run against whatever source path is provided.

8. **Hooks format**: Copilot uses standalone `hooks.json` while Claude stores hooks inside `settings.json`. **Decision**: The `CopilotHookDetector` reads standalone `hooks.json` files. Hook event names are normalized from legacy names (e.g., `SessionStart` → `sessionStart`) to canonical names during migration.

9. **Copilot Extensions**: `.github/extensions/` is a new Copilot component type (child processes). **Decision**: Include in scope. Add `ArtifactKind::Extension` and a `CopilotExtensionDetector`.

10. **Copilot agent file extensions**: Both `.md` and `.agent.md` are accepted by Copilot runtime. **Decision**: Detect both with dedup logic — `.agent.md` takes precedence when both exist for the same agent name.

11. **ToolAdaptor for `aipm init`**: **Decision**: Deferred to a separate spec. This spec covers migrate-only (reading Copilot configs). The init adaptor (writing Copilot configs) is a separate concern.
