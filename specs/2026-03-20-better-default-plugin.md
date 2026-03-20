# Better Default Plugin for `aipm init`

| Document Metadata      | Details                                                                                     |
| ---------------------- | ------------------------------------------------------------------------------------------- |
| Author(s)              | selarkin                                                                                    |
| Status                 | Draft (WIP)                                                                                 |
| Team / Owner           | AI Dev Tooling                                                                              |
| Created / Last Updated | 2026-03-20                                                                                  |
| Research               | [research/docs/2026-03-20-30-better-default-plugin.md](../research/docs/2026-03-20-30-better-default-plugin.md) |
| Issue                  | GitHub Issue #30                                                                            |

## 1. Executive Summary

This spec replaces the minimal "starter" plugin scaffolded by `aipm init` with a functional default plugin containing three real components: (1) a scaffold-plugin skill that runs a TypeScript script (via Node's `--experimental-strip-types`) to create new plugins in `.ai/`, (2) a marketplace-scanner sub-agent scoped to read-only analysis of the `.ai/` directory, and (3) a post-model logging hook. A new `--no-starter` CLI flag allows users to skip the starter plugin entirely, creating only the bare `.ai/` directory. All markdown files stay under 50 lines. Changes touch `scaffold_marketplace()` and its `generate_*()` helpers in `workspace_init/mod.rs`, plus the `Options` struct and CLI argument parsing for the new flag.

## 2. Context and Motivation

### 2.1 Current State

`aipm init --marketplace` scaffolds a `.ai/starter/` plugin with a single placeholder skill and empty `agents/` and `hooks/` directories ([research §Current Starter Plugin](../research/docs/2026-03-20-30-better-default-plugin.md)):

```
.ai/
  .gitignore
  starter/
    aipm.toml                    # [components] skills = ["skills/hello/SKILL.md"]
    .claude-plugin/plugin.json
    .mcp.json
    skills/hello/SKILL.md        # 12-line generic placeholder
    agents/.gitkeep              # empty
    hooks/.gitkeep               # empty
```

The `SKILL.md` contains boilerplate instructions with no real functionality. Agents and hooks directories are stubs. Users get no working examples of these component types after init.

### 2.2 The Problem

| Problem | Impact |
|---------|--------|
| Starter skill is a generic placeholder | Users don't learn how skills work or how to create plugins |
| No agent example | Users have no reference for agent scoping (`tools` frontmatter) or markdown agent format |
| No hook example | Users don't see how hooks.json lifecycle events work |
| `.gitkeep` stubs signal "not ready" | Sends the wrong message about the platform's capabilities |

## 3. Goals and Non-Goals

### 3.1 Functional Goals

- [ ] Replace `skills/hello/SKILL.md` with `skills/scaffold-plugin/SKILL.md` — a skill that invokes a TypeScript script to scaffold a new plugin directory in `.ai/`
- [ ] Add `scripts/scaffold-plugin.ts` — a small TypeScript script (run via `node --experimental-strip-types`) that creates the plugin directory structure
- [ ] Replace `agents/.gitkeep` with `agents/marketplace-scanner.md` — a sub-agent scoped to read-only `.ai/` directory analysis
- [ ] Replace `hooks/.gitkeep` with `hooks/hooks.json` — a hook providing basic logging after model runs
- [ ] Update `generate_starter_manifest()` to declare all four components: skills, agents, hooks, scripts
- [ ] Update `generate_plugin_json()` description to reflect the new plugin purpose
- [ ] Keep all markdown files under 50 lines each
- [ ] All new files must be written before `parse_and_validate()` call (write-before-validate ordering)
- [ ] Add `--no-starter` CLI flag to `aipm init` that skips the starter plugin (creates bare `.ai/` directory with only `.gitignore`)
- [ ] Thread the `no_starter` option through `Options` struct and `scaffold_marketplace()`
- [ ] All `cargo build/test/clippy/fmt` must pass with zero warnings
- [ ] Update BDD scenarios to assert new file paths and `--no-starter` behavior

### 3.2 Non-Goals (Out of Scope)

- [ ] Renaming the plugin from "starter" — keep the existing name for backwards compatibility with docs
- [ ] Bundling a TypeScript runtime — the script requires Node.js >= 22.6.0 with `--experimental-strip-types` support; no third-party runners needed
- [ ] Implementing the P2 quality lint guardrails for agents/hooks — those are separate issues
- [ ] Changes to the `ToolAdaptor` trait, `ClaudeAdaptor`, or `.claude/settings.json` generation
- [ ] Changes to the manifest schema (`types.rs`) or validation logic (`validate.rs`)

## 4. Proposed Solution (High-Level Design)

### 4.1 New Directory Tree

After `aipm init --marketplace`, the `.ai/starter/` plugin will contain:

```
.ai/
  .gitignore
  starter/
    aipm.toml                              # Updated [components] with all four types
    .claude-plugin/plugin.json             # Updated description
    .mcp.json                              # Unchanged
    skills/scaffold-plugin/SKILL.md        # NEW — scaffold-plugin skill
    scripts/scaffold-plugin.ts             # NEW — TypeScript scaffolding script
    agents/marketplace-scanner.md          # NEW — read-only .ai/ scanner agent
    hooks/hooks.json                       # NEW — post-model logging hook
```

After `aipm init --marketplace --no-starter`, only the bare marketplace directory is created:

```
.ai/
  .gitignore
```

### 4.2 Architectural Pattern

**Template Generation** — follows the existing pattern established by `generate_skill_template()`, `generate_starter_manifest()`, etc. Each new component gets a dedicated `generate_*()` function that returns a `String`. All content is written to disk before manifest validation.

### 4.3 Key Components

| Component | File | Purpose | Technology |
|-----------|------|---------|------------|
| Scaffold-Plugin Skill | `skills/scaffold-plugin/SKILL.md` | Instructs the AI to run the scaffolding script when user wants to create a new plugin | Markdown with YAML frontmatter |
| Scaffold Script | `scripts/scaffold-plugin.ts` | Creates a new plugin directory with `aipm.toml`, skill stub, and `.claude-plugin/` | TypeScript (run via `node --experimental-strip-types`) |
| Marketplace Scanner | `agents/marketplace-scanner.md` | Sub-agent scoped to read-only analysis of `.ai/` directory contents | Markdown with YAML frontmatter |
| Logging Hook | `hooks/hooks.json` | Logs a timestamp and tool name after every model tool use | JSON hook config |

## 5. Detailed Design

### 5.1 Scaffold-Plugin Skill — `skills/scaffold-plugin/SKILL.md`

This skill has `disable-model-invocation: false` (default) because the AI needs to interpret user intent (plugin name, description) before invoking the script. The skill instructs the agent to gather parameters and run the TypeScript script.

**Content (under 50 lines):**

```markdown
---
description: Scaffold a new AI plugin in the .ai/ marketplace directory. Use when the user wants to create a new plugin, skill, agent, or hook package.
---

# Scaffold Plugin

Create a new plugin in the `.ai/` marketplace directory.

## Instructions

1. Ask the user for a plugin name (lowercase, hyphens allowed) if not provided.
2. Run the scaffolding script:
   ```bash
   node --experimental-strip-types .ai/starter/scripts/scaffold-plugin.ts <plugin-name>
   ```
3. Report the created file tree to the user.
4. Suggest next steps: edit the generated `SKILL.md`, add agents or hooks, update `aipm.toml`.

## Notes

- The script creates `.ai/<plugin-name>/` with a valid `aipm.toml` and starter skill.
- If the directory already exists, the script exits with an error message.
- After scaffolding, the user should customize the generated files.
```

### 5.2 Scaffold Script — `scripts/scaffold-plugin.ts`

A minimal TypeScript script that creates a new plugin directory. It runs via `node --experimental-strip-types` (Node.js >= 22.6.0) — no third-party TypeScript runners required. It must not use any npm dependencies — only Node.js built-ins (`fs`, `path`, `process`).

**Content:**

```typescript
import { mkdirSync, writeFileSync, existsSync } from "fs";
import { join } from "path";

const name = process.argv[2];
if (!name) {
  process.stderr.write("Usage: node --experimental-strip-types scaffold-plugin.ts <plugin-name>\n");
  process.exit(1);
}

const aiDir = join(process.cwd(), ".ai");
const pluginDir = join(aiDir, name);

if (existsSync(pluginDir)) {
  process.stderr.write(`Error: .ai/${name}/ already exists\n`);
  process.exit(1);
}

mkdirSync(join(pluginDir, ".claude-plugin"), { recursive: true });
mkdirSync(join(pluginDir, "skills", name), { recursive: true });
mkdirSync(join(pluginDir, "agents"), { recursive: true });
mkdirSync(join(pluginDir, "hooks"), { recursive: true });

writeFileSync(
  join(pluginDir, "aipm.toml"),
  `[package]\nname = "${name}"\nversion = "0.1.0"\ntype = "composite"\nedition = "2024"\ndescription = "TODO: describe ${name}"\n\n[components]\nskills = ["skills/${name}/SKILL.md"]\n`
);

writeFileSync(
  join(pluginDir, "skills", name, "SKILL.md"),
  `---\ndescription: TODO — describe when this skill should be invoked\n---\n\n# ${name}\n\nReplace this with instructions for the AI agent.\n`
);

writeFileSync(
  join(pluginDir, ".claude-plugin", "plugin.json"),
  JSON.stringify({ name, version: "0.1.0", description: `TODO: describe ${name}` }, null, 2) + "\n"
);

process.stdout.write(`Created .ai/${name}/ with starter structure\n`);
```

### 5.3 Marketplace Scanner Agent — `agents/marketplace-scanner.md`

Scoped to read-only tools only. The `tools` frontmatter key restricts the agent to `Read`, `Glob`, `Grep`, and `LS` — no `Edit`, `Write`, or `Bash` access. This follows the agent markdown format documented in [Claude Code defaults research §7](../research/docs/2026-03-16-claude-code-defaults.md) and [Copilot agent discovery research](../research/docs/2026-03-16-copilot-agent-discovery.md).

**Content (under 50 lines):**

```markdown
---
name: marketplace-scanner
description: Scan and explain the contents of the .ai/ marketplace directory. Use when the user wants to understand what plugins, skills, agents, or hooks are installed locally.
tools:
  - Read
  - Glob
  - Grep
  - LS
---

# Marketplace Scanner

You are a read-only analysis agent for the `.ai/` marketplace directory.

## Instructions

1. List all plugin directories under `.ai/` (each subdirectory with an `aipm.toml`).
2. For each plugin, read its `aipm.toml` and summarize:
   - Package name, version, type, and description
   - Declared components (skills, agents, hooks, scripts)
3. If asked about a specific component, read and explain its contents.
4. Never modify any files — you are read-only.

## Scope

- Only scan files within the `.ai/` directory.
- Do not access files outside `.ai/` unless explicitly asked.
- Report any `aipm.toml` parse issues you encounter.
```

### 5.4 Logging Hook — `hooks/hooks.json`

Uses the `PostToolUse` lifecycle event ([Claude Code defaults research §7](../research/docs/2026-03-16-claude-code-defaults.md)). The hook appends a timestamped log line to `.ai/.tool-usage.log`.

**Content:**

```json
{
  "hooks": [
    {
      "event": "PostToolUse",
      "command": "echo \"$(date -u +%Y-%m-%dT%H:%M:%SZ) tool=$TOOL_NAME\" >> .ai/.tool-usage.log"
    }
  ]
}
```

> **Note:** The `PostToolUse` event and `$TOOL_NAME` environment variable are Claude Code hook conventions. The command is a simple shell one-liner that requires no dependencies. On Windows, this will need a compatible shell (Git Bash, WSL) — this is acceptable for a starter template.

### 5.5 Updated Starter Manifest — `generate_starter_manifest()`

The manifest grows from declaring only `skills` to declaring all four component types:

```toml
[package]
name = "starter"
version = "0.1.0"
type = "composite"
edition = "2024"
description = "Default starter plugin — scaffold new plugins, scan your marketplace, and log tool usage"

# [dependencies]
# Add registry dependencies here, e.g.:
# shared-skill = "^1.0"

[components]
skills = ["skills/scaffold-plugin/SKILL.md"]
agents = ["agents/marketplace-scanner.md"]
hooks = ["hooks/hooks.json"]
scripts = ["scripts/scaffold-plugin.ts"]
```

### 5.6 Updated Plugin JSON — `generate_plugin_json()`

```json
{
  "name": "starter",
  "version": "0.1.0",
  "description": "Default starter plugin — scaffold new plugins, scan your marketplace, and log tool usage"
}
```

### 5.7 Changes to `scaffold_marketplace()`

The function's directory creation and file writing steps change as follows:

**Directory creation (replace existing):**

```rust
// Create directory tree
std::fs::create_dir_all(starter.join(".claude-plugin"))?;
std::fs::create_dir_all(starter.join("skills").join("scaffold-plugin"))?;
std::fs::create_dir_all(starter.join("scripts"))?;
std::fs::create_dir_all(starter.join("agents"))?;
std::fs::create_dir_all(starter.join("hooks"))?;
```

**File writes before validation (all must precede `parse_and_validate()`):**

```rust
// Write all component files before manifest validation
write_file(&starter.join("skills").join("scaffold-plugin").join("SKILL.md"),
           &generate_skill_template())?;
write_file(&starter.join("scripts").join("scaffold-plugin.ts"),
           &generate_scaffold_script())?;
write_file(&starter.join("agents").join("marketplace-scanner.md"),
           &generate_agent_template())?;
write_file(&starter.join("hooks").join("hooks.json"),
           &generate_hook_template())?;

// Write manifest and validate
let starter_manifest = generate_starter_manifest();
write_file(&starter.join("aipm.toml"), &starter_manifest)?;
crate::manifest::parse_and_validate(&starter_manifest, Some(&starter))
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
```

**File writes after validation (metadata files):**

```rust
// .ai/starter/.claude-plugin/plugin.json
write_file(&starter.join(".claude-plugin").join("plugin.json"), &generate_plugin_json())?;

// .ai/starter/.mcp.json
write_file(&starter.join(".mcp.json"), &generate_mcp_stub())?;
```

**Removals:**

- Delete the two `write_file` calls for `.gitkeep` files (lines 206-207)
- Delete `skills/hello/` directory creation (line 175)

### 5.8 New `generate_*()` Functions

Four functions are modified or added:

| Function | Status | Description |
|----------|--------|-------------|
| `generate_starter_manifest()` | **Modified** | Updated `[components]` to include skills, agents, hooks, scripts |
| `generate_plugin_json()` | **Modified** | Updated description |
| `generate_skill_template()` | **Modified** | Replaced hello skill with scaffold-plugin skill content |
| `generate_scaffold_script()` | **New** | Returns the TypeScript script as a string |
| `generate_agent_template()` | **New** | Returns the marketplace-scanner agent markdown |
| `generate_hook_template()` | **New** | Returns the hooks.json content |
| `generate_mcp_stub()` | **Unchanged** | No changes |

### 5.9 `--no-starter` CLI Flag

The `--no-starter` flag allows users to create the `.ai/` marketplace directory without the starter plugin. This is useful for teams that want to manage their own plugin structure from scratch.

#### 5.9.1 CLI Changes — `crates/aipm/src/main.rs`

Add a new argument to the `Init` variant:

```rust
/// Initialize a workspace for AI plugin management.
Init {
    /// Generate a workspace manifest (aipm.toml with [workspace] section).
    #[arg(long)]
    workspace: bool,

    /// Generate a .ai/ local marketplace with tool settings.
    #[arg(long)]
    marketplace: bool,

    /// Skip the starter plugin (create bare .ai/ directory only).
    #[arg(long)]
    no_starter: bool,

    /// Directory to initialize (defaults to current directory).
    #[arg(default_value = ".")]
    dir: PathBuf,
},
```

Pass the flag through to `Options`:

```rust
let opts = libaipm::workspace_init::Options {
    dir: &dir,
    workspace: do_workspace,
    marketplace: do_marketplace,
    no_starter,
};
```

Update the `MarketplaceCreated` output message to reflect the mode:

```rust
libaipm::workspace_init::InitAction::MarketplaceCreated => {
    if no_starter {
        "Created .ai/ marketplace (no starter plugin)".to_string()
    } else {
        "Created .ai/ marketplace with starter plugin".to_string()
    }
},
```

#### 5.9.2 Options Struct — `workspace_init/mod.rs`

Add `no_starter` to `Options`:

```rust
/// Options for workspace initialization.
pub struct Options<'a> {
    /// Target directory.
    pub dir: &'a Path,
    /// Generate workspace manifest.
    pub workspace: bool,
    /// Generate `.ai/` marketplace + tool settings.
    pub marketplace: bool,
    /// Skip the starter plugin (bare `.ai/` directory only).
    pub no_starter: bool,
}
```

#### 5.9.3 `scaffold_marketplace()` Signature Change

The function gains a `no_starter: bool` parameter:

```rust
fn scaffold_marketplace(dir: &Path, no_starter: bool) -> Result<(), Error> {
    let ai_dir = dir.join(".ai");
    if ai_dir.exists() {
        return Err(Error::MarketplaceAlreadyExists(dir.to_path_buf()));
    }

    // Always create .ai/ and .gitignore
    std::fs::create_dir_all(&ai_dir)?;
    write_file(
        &ai_dir.join(".gitignore"),
        "# Managed by aipm — registry-installed plugins are symlinked here.\n\
         # Do not edit the section between the markers.\n\
         # === aipm managed start ===\n\
         # === aipm managed end ===\n",
    )?;

    if no_starter {
        return Ok(());
    }

    // ... existing starter plugin scaffolding continues here ...
}
```

The caller in `init()` passes the flag:

```rust
if opts.marketplace {
    scaffold_marketplace(opts.dir, opts.no_starter)?;
    actions.push(InitAction::MarketplaceCreated);
    // ...
}
```

#### 5.9.4 Flag Interactions

| Flags | Behavior |
|-------|----------|
| `aipm init` (no flags) | `--marketplace` implied, starter plugin created |
| `aipm init --marketplace` | Starter plugin created |
| `aipm init --marketplace --no-starter` | Bare `.ai/` + `.gitignore` only, no `starter/` |
| `aipm init --no-starter` | Same as above (`--marketplace` implied) |
| `aipm init --workspace --no-starter` | Only `aipm.toml`, no `.ai/` directory at all (`--no-starter` has no effect without `--marketplace`) |
| `aipm init --workspace --marketplace --no-starter` | `aipm.toml` + bare `.ai/` |

> **Note:** `--no-starter` only has meaning when `--marketplace` is active (explicitly or by default). When only `--workspace` is passed, the flag is silently ignored since no marketplace scaffolding occurs.

## 6. Alternatives Considered

| Option | Pros | Cons | Reason for Rejection |
|--------|------|------|---------------------|
| **Shell script instead of TypeScript** | No runtime dependency | Less readable, harder to maintain, poor Windows compat | TypeScript is more idiomatic for AI tool ecosystems; Node.js `--experimental-strip-types` requires no extra tooling |
| **`npx tsx` or `ts-node` runner** | Well-known, stable API | Adds a third-party dependency or network fetch; slower cold start | `node --experimental-strip-types` is zero-dependency and ships with Node.js >= 22.6.0 |
| **Inline skill without script** | Simpler, no scripts/ directory needed | AI would generate the file structure ad-hoc each time; less consistent | A script ensures deterministic output and teaches the scripts component pattern |
| **`disable-model-invocation: true` on scaffold skill** | Prevents AI from using tokens when scaffolding | The AI needs to interpret user intent (plugin name, description) and report results | Model invocation is necessary for a good UX |
| **Separate plugin per component** | Demonstrates single-type plugin format | More directories, confusing for first-time users | A single composite plugin is simpler to understand |
| **Log to stdout instead of file** | Simpler hook command | Ephemeral, user can't review history | A log file provides persistent, reviewable history |
| **`--starter` opt-in instead of `--no-starter` opt-out** | Explicit consent for starter content | Breaks existing behavior; new users miss the examples | Starter plugin is educational — opt-out is the right default |

## 7. Cross-Cutting Concerns

### 7.1 Validation

- All four component paths (`skills/scaffold-plugin/SKILL.md`, `agents/marketplace-scanner.md`, `hooks/hooks.json`, `scripts/scaffold-plugin.ts`) must exist on disk before `parse_and_validate()` is called — matching the existing write-before-validate pattern ([research §Validation Considerations](../research/docs/2026-03-20-30-better-default-plugin.md))
- `validate_component_paths()` at `validate.rs:167-192` already handles `scripts` validation via the `all_paths` array — no validation code changes needed
- The manifest round-trip test must be updated to create all four component files in the temp directory

### 7.2 50-Line Constraint

| File | Estimated Lines | Under 50? |
|------|----------------|-----------|
| `skills/scaffold-plugin/SKILL.md` | ~22 | Yes |
| `agents/marketplace-scanner.md` | ~26 | Yes |
| `hooks/hooks.json` | ~8 | Yes |
| `scripts/scaffold-plugin.ts` | ~35 | Yes |

### 7.3 Platform Considerations

- The hook's `echo` + `date` command uses Unix shell syntax. On Windows with Git Bash or WSL this works. On native Windows cmd.exe it will not — this is acceptable for a starter template that users are expected to customize.
- The scaffold script uses `process.exit(1)` — note this is in the generated TypeScript file, not in aipm's Rust code, so the `forbid(exit)` lint does not apply.
- `node --experimental-strip-types` requires Node.js >= 22.6.0. This is the current LTS line (as of 2026). The flag emits a warning to stderr on older Node versions but does not fail — however, type stripping may not work correctly below 22.6.0. The skill instructions should note the minimum Node version.

## 8. Migration, Rollout, and Testing

### 8.1 Deployment Strategy

- [ ] **Phase 1**: Add `no_starter: bool` to `Options` struct and thread it through `init()` → `scaffold_marketplace()`. Add `--no-starter` CLI arg. Existing behavior unchanged (flag defaults to `false`).
- [ ] **Phase 2**: Add new `generate_*()` functions (`generate_scaffold_script()`, `generate_agent_template()`, `generate_hook_template()`). No functional changes yet — just new dead code.
- [ ] **Phase 3**: Update `generate_starter_manifest()` and `generate_plugin_json()` with new content.
- [ ] **Phase 4**: Update `scaffold_marketplace()` — new directory tree, new file writes, remove `.gitkeep` writes, early return when `no_starter` is true.
- [ ] **Phase 5**: Update `generate_skill_template()` — replace hello skill with scaffold-plugin content.
- [ ] **Phase 6**: Update unit tests and BDD scenarios (including `--no-starter` cases).
- [ ] **Phase 7**: Run full `cargo build --workspace && cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check`.

### 8.2 Test Plan

**Unit tests to modify (`workspace_init/mod.rs`):**

| Test | Change |
|------|--------|
| `starter_manifest_round_trips` | Create all four component files in temp dir (not just `skills/hello/SKILL.md`) |
| `init_marketplace_creates_tree` | Assert new file paths (`scaffold-plugin/SKILL.md`, `marketplace-scanner.md`, `hooks.json`, `scaffold-plugin.ts`); remove `.gitkeep` assertions |
| `skill_template_has_frontmatter` | Update assertion — content changes but `description:` frontmatter still present |
| All tests constructing `Options` | Add `no_starter: false` to existing `Options` structs |

**New unit tests:**

| Test | Purpose |
|------|---------|
| `agent_template_has_frontmatter` | Verify agent markdown contains `name:`, `description:`, `tools:` frontmatter |
| `hook_template_is_valid_json` | Verify `generate_hook_template()` parses as valid JSON with a `hooks` array |
| `scaffold_script_is_nonempty` | Verify `generate_scaffold_script()` is non-empty and contains expected markers |
| `init_marketplace_no_starter` | Verify `no_starter: true` creates `.ai/` + `.gitignore` but no `starter/` directory |
| `init_no_starter_still_configures_tools` | Verify tool adaptors still run when `--no-starter` is set (`.claude/settings.json` is created) |

**BDD scenarios to modify (`tests/features/manifest/workspace-init.feature`):**

| Scenario | Change |
|----------|--------|
| "Marketplace generates a Claude Code plugin structure" (line 37-42) | Replace `skills/hello/SKILL.md` assertion with `skills/scaffold-plugin/SKILL.md`; add assertions for `agents/marketplace-scanner.md`, `hooks/hooks.json`, `scripts/scaffold-plugin.ts` |
| "Marketplace generates a starter skill with description frontmatter" (line 44-48) | Update path from `skills/hello/SKILL.md` to `skills/scaffold-plugin/SKILL.md` |
| "Initialize a workspace with default marketplace" (line 15-27) | Add `scripts/` to the directory list |

**BDD scenarios to add:**

| Scenario | Purpose |
|----------|---------|
| "Starter plugin includes a marketplace scanner agent" | Assert `agents/marketplace-scanner.md` exists and contains `tools:` frontmatter |
| "Starter plugin includes a logging hook" | Assert `hooks/hooks.json` exists and contains `PostToolUse` |
| "Starter plugin includes a scaffold script" | Assert `scripts/scaffold-plugin.ts` exists |
| "No-starter flag creates bare marketplace directory" | Assert `.ai/` and `.ai/.gitignore` exist but `.ai/starter/` does not |
| "No-starter flag still configures tool settings" | Assert `.claude/settings.json` is created even with `--no-starter` |
| "No-starter flag with workspace creates both minus starter" | Assert `aipm.toml` + `.ai/` exist but no `.ai/starter/` |

## 9. Open Questions / Unresolved Issues

- [x] **TypeScript runtime assumption**: **RESOLVED** — Use `node --experimental-strip-types` (Node.js >= 22.6.0). Zero third-party dependencies; no `npx tsx` or `ts-node` needed. The flag is stable enough for a starter template, and Node.js 22 is the current LTS.
- [ ] **Hook event name**: The research documents `PreToolUse` and `PostToolUse` but does not confirm these are the only events. The issue says "after any model runs" which might imply a broader event. **Recommendation**: Use `PostToolUse` as documented — it fires after each tool use which is the closest available event.
- [ ] **`.ai/.tool-usage.log` in `.gitignore`**: Should the managed `.ai/.gitignore` include `*.log` or `.tool-usage.log` to prevent accidental commits of the log file? **Recommendation**: Yes — add `.tool-usage.log` to the gitignore managed section.
- [ ] **Windows hook compatibility**: The `echo` + `date` hook command won't work on native Windows cmd.exe. Should we provide a cross-platform alternative (e.g., a Node.js one-liner)? **Recommendation**: Accept the limitation for now — document it in the hook file with a comment. Users on Windows with Git Bash/WSL will be fine.
