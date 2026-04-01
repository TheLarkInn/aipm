---
date: 2026-03-31 14:33:51 UTC
researcher: Claude
git_commit: 1b8483daae7b50608a93a114404330d1e235d222
branch: main
repository: aipm
topic: "Comprehensive architecture research for aipm lint command (GitHub Issue #110)"
tags: [research, codebase, lint, adapter, migrate, detector, plugin, marketplace, validation, rules, cli, configuration]
status: complete
last_updated: 2026-03-31
last_updated_by: Claude
---

# Research: `aipm lint` Architecture — Issue #110 (Comprehensive)

## Research Question

Document the current codebase architecture relevant to building `aipm lint` -- specifically: (1) the adapter architecture used by `aipm migrate`, (2) existing CLI command structure and how new commands are added, (3) any existing validation/rule patterns, (4) the plugin/marketplace directory structure and conventions that lint rules would enforce, and (5) how the project handles configuration files.

## Summary

The codebase provides a mature **trait-based strategy pattern** in the migrate pipeline (`Detector` trait) that directly maps to a lint rule architecture. Twelve concrete detectors across two tool families (Claude Code, Copilot CLI) demonstrate the adapter pattern for multi-tool support. The CLI uses clap derive macros with a flat `Commands` enum -- adding a new subcommand requires three changes: enum variant, handler function, match arm. Manifest validation already implements error accumulation via `Error::Multiple(Vec<Self>)`, and the security module has a binary `ScriptVerdict` pattern. Configuration is handled through `aipm.toml` (serde + toml crate), with `toml_edit` for comment-preserving edits. The BDD feature file at `tests/features/guardrails/quality.feature` specifies expected lint behavior including severity levels, `--fix` mode, publish gating, and quality scoring.

---

## Detailed Findings

### 1. Migrate Adapter (Detector) Architecture

The migrate pipeline uses a **strategy pattern** with trait objects for scanning tool-specific configuration directories and converting artifacts into a unified plugin format.

#### The Detector Trait

[`crates/libaipm/src/migrate/detector.rs:9-20`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/detector.rs#L9-L20)

```rust
pub trait Detector {
    fn name(&self) -> &'static str;
    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error>;
}
```

- Takes a `source_dir` path and an `&dyn Fs` filesystem abstraction
- Returns zero or more `Artifact` values (the findings)
- Each detector is a zero-sized unit struct implementing this trait

#### Detector Factory Functions

[`crates/libaipm/src/migrate/detector.rs:23-53`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/detector.rs#L23-L53)

Two factory functions create detector sets:

- **`claude_detectors()`** (lines 23-32): Returns 6 detectors for `.claude/` sources
- **`copilot_detectors()`** (lines 35-44): Returns 6 detectors for `.github/` sources
- **`detectors_for_source()`** (lines 47-53): Dispatches on source type string: `".claude"` -> `claude_detectors()`, `".github"` -> `copilot_detectors()`, else empty vec

#### The 12 Detector Implementations

**Claude Code Detectors (`.claude/` source):**

| Struct | File | Scans | Produces |
|--------|------|-------|----------|
| `SkillDetector` | [`skill_detector.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/skill_detector.rs) | `.claude/skills/<name>/SKILL.md` | `ArtifactKind::Skill` |
| `CommandDetector` | [`command_detector.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/command_detector.rs) | `.claude/commands/*.md` | `ArtifactKind::Command` |
| `AgentDetector` | [`agent_detector.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/agent_detector.rs) | `.claude/agents/*.md` | `ArtifactKind::Agent` |
| `McpDetector` | [`mcp_detector.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/mcp_detector.rs) | `.mcp.json` at project root | `ArtifactKind::McpServer` |
| `HookDetector` | [`hook_detector.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/hook_detector.rs) | `.claude/settings.json` hooks key | `ArtifactKind::Hook` |
| `OutputStyleDetector` | [`output_style_detector.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/output_style_detector.rs) | `.claude/output-styles/*.md` | `ArtifactKind::OutputStyle` |

**Copilot CLI Detectors (`.github/` source):**

| Struct | File | Scans | Produces |
|--------|------|-------|----------|
| `CopilotSkillDetector` | [`copilot_skill_detector.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/copilot_skill_detector.rs) | `.github/skills/<name>/SKILL.md` | `ArtifactKind::Skill` |
| `CopilotAgentDetector` | [`copilot_agent_detector.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/copilot_agent_detector.rs) | `.github/agents/<name>.md` or `<name>.agent.md` | `ArtifactKind::Agent` |
| `CopilotMcpDetector` | [`copilot_mcp_detector.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/copilot_mcp_detector.rs) | `.copilot/mcp-config.json` | `ArtifactKind::McpServer` |
| `CopilotHookDetector` | [`copilot_hook_detector.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/copilot_hook_detector.rs) | `.github/hooks.json` or `.github/hooks/hooks.json` | `ArtifactKind::Hook` |
| `CopilotExtensionDetector` | [`copilot_extension_detector.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/copilot_extension_detector.rs) | `.github/extensions/<name>/` | `ArtifactKind::Extension` |
| `CopilotLspDetector` | [`copilot_lsp_detector.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/copilot_lsp_detector.rs) | `.github/lsp.json` | `ArtifactKind::LspServer` |

#### Common Data Types

[`crates/libaipm/src/migrate/mod.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/mod.rs)

- **`ArtifactKind`** (lines 29-47): Enum with 8 variants: `Skill`, `Command`, `Agent`, `McpServer`, `Hook`, `OutputStyle`, `LspServer`, `Extension`
- **`Artifact`** (lines 96-110): Universal intermediate representation with `kind`, `name`, `source_path`, `files`, `referenced_scripts`, `metadata`
- **`ArtifactMetadata`** (lines 66-79): Contains `name`, `description`, `hooks`, `model_invocation_disabled`, `raw_content`

#### Two-Phase Pipeline

1. **Detection phase** (read-only, parallel-safe via `rayon::par_iter()`): produces `Artifact` values
2. **Emission phase** (writes): consumes artifacts to create plugin directories

The lint command would only need a detection-like phase (read-only scan producing diagnostics).

#### Recursive Discovery

[`crates/libaipm/src/migrate/discovery.rs:54-76`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/discovery.rs#L54-L76)

Uses `ignore` crate for gitignore-aware walking. `discover_source_dirs()` accepts patterns like `[".claude", ".github"]` and returns `DiscoveredSource` structs. Explicitly filters out `.ai/` directories.

#### Filesystem Abstraction

[`crates/libaipm/src/fs.rs:25-39`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/fs.rs#L25-L39)

The `Fs` trait (`Send + Sync`) provides `exists`, `create_dir_all`, `write_file`, `read_to_string`, `read_dir`. Production uses `fs::Real`; tests use per-module `MockFs` structs.

#### Shared Frontmatter Parsing

[`crates/libaipm/src/migrate/skill_common.rs:13-77`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/skill_common.rs#L13-L77)

`parse_skill_frontmatter()` does line-by-line scanning for `---` delimiters and key-value extraction. Both Claude and Copilot skill detectors delegate to this shared function, differing only in script reference variable prefixes (`${CLAUDE_SKILL_DIR}/` vs `${SKILL_DIR}/`).

---

### 2. CLI Command Structure

The project has two CLI binaries sharing business logic through `libaipm`.

#### Project Layout

[`Cargo.toml:1-2`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/Cargo.toml#L1-L2)

| Crate | Purpose |
|-------|---------|
| `crates/aipm/` | Consumer CLI binary |
| `crates/aipm-pack/` | Author CLI binary |
| `crates/libaipm/` | Shared library |

#### Consumer CLI: `aipm`

[`crates/aipm/src/main.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/aipm/src/main.rs)

Top-level struct (lines 13-18):
```rust
#[derive(Parser)]
#[command(name = "aipm", version = libaipm::version(), about = "AI Plugin Manager — consumer CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}
```

**`Commands` enum** (lines 20-141) with 7 subcommands:

| Subcommand | Handler | Lines | Calls into `libaipm` |
|------------|---------|-------|---------------------|
| `Init` | `cmd_init()` | 247-294 | `libaipm::workspace_init::init()` |
| `Install` | `cmd_install()` | 296-336 | `libaipm::installer::pipeline::install()` |
| `Update` | `cmd_update()` | 338-364 | `libaipm::installer::pipeline::update()` |
| `Link` | `cmd_link()` | 366-409 | `libaipm::linker::directory_link::create()` |
| `Unlink` | `cmd_unlink()` | 411-432 | `libaipm::linker::pipeline::unlink_package()` |
| `List` | `cmd_list()` | 434-476 | `libaipm::linker::link_state::list()` |
| `Migrate` | `cmd_migrate()` | 478-558 | `libaipm::migrate::migrate()` |

Dispatch at lines 564-596: `run()` calls `Cli::parse()`, then `match cli.command`.

#### Author CLI: `aipm-pack`

[`crates/aipm-pack/src/main.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/aipm-pack/src/main.rs)

Same pattern, currently only has `Init` command (lines 15-42). The BDD feature file specifies `aipm-pack lint`, suggesting lint belongs here (or in both CLIs).

#### Pattern for Adding a New CLI Subcommand

1. Add a variant to the `Commands` enum with `#[arg]` annotations on fields
2. Add a `cmd_*()` handler function that calls `resolve_dir()`, invokes `libaipm`, and writes output to stdout
3. Add a match arm in `run()` that destructures the variant and calls the handler
4. Implement actual logic in a `libaipm` module
5. If interactive prompts needed, add to `wizard.rs` (pure logic) and `wizard_tty.rs` (TTY bridge)

#### Interactive Wizard System

[`crates/aipm/src/wizard.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/aipm/src/wizard.rs)

Split into pure logic (`wizard.rs` -- testable, defines `PromptStep`/`PromptKind`/`PromptAnswer`) and TTY bridge (`wizard_tty.rs` -- calls `inquire` crate, excluded from coverage). Interactivity check: `!flags.yes && std::io::stdin().is_terminal()`.

#### Shared CLI Helpers

All in `crates/aipm/src/main.rs`:

- `resolve_dir()` (lines 175-181): Converts `"."` to `current_dir()`
- `resolve_plugins_dir()` (lines 185-195): Reads `[workspace].plugins_dir` from `aipm.toml`, falls back to `.ai`
- `home_store_path()` (lines 198-203): Returns `~/.aipm/store/`
- `timestamp_now()` (lines 208-221): Approximate ISO-8601 UTC timestamp
- `StubRegistry` (lines 147-168): Placeholder `Registry` trait impl

---

### 3. Existing Validation and Rule-Based Patterns

#### Manifest Validation (Error Accumulation)

[`crates/libaipm/src/manifest/validate.rs:76-111`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/manifest/validate.rs#L76-L111)

The `validate()` function collects errors into a `Vec<Error>`, dispatching to three sub-validators:

1. **`validate_package()`** (line 113): name non-empty + valid pattern, version semver, plugin type
2. **`validate_dependencies()`** (line 140): each dep version string parseable
3. **`validate_component_paths()`** (line 172): every declared component path exists on disk

Aggregation pattern (lines 103-110): if 1 error, return directly; if multiple, wrap in `Error::Multiple(Vec<Self>)`.

Name validation at line 14 enforces `^(@[a-z0-9-]+/)?[a-z0-9][a-z0-9-]*$`.

#### Manifest Error Types

[`crates/libaipm/src/manifest/error.rs:10-84`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/manifest/error.rs#L10-L84)

9 variants: `Parse`, `MissingField`, `InvalidName`, `InvalidVersion`, `InvalidDependencyVersion`, `InvalidPluginType`, `InvalidWorkspaceProtocol`, `ComponentNotFound`, `Io`, `Multiple`.

#### Security Verdict Pattern (Binary Severity)

[`crates/libaipm/src/linker/security.rs:20-26`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/linker/security.rs#L20-L26)

`ScriptVerdict` enum: `Allowed` or `Blocked`. `evaluate_scripts()` checks each script against an allowlist, returning `Vec<(LifecycleScript, ScriptVerdict)>`. Helpers: `has_blocked_scripts()`, `blocked_scripts()`. This is the closest existing pattern to "rule produces verdict with severity."

#### Action-Based Diagnostics in Migrate

[`crates/libaipm/src/migrate/mod.rs:133-187`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/mod.rs#L133-L187)

Rather than errors, the migration pipeline produces `Action` variants including `Skipped { name, reason }` and `Renamed { original_name, new_name, reason }` -- informational diagnostics mixed into the action list alongside success actions.

#### Path Safety Check in Emitter

[`crates/libaipm/src/migrate/emitter.rs:17-24`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/emitter.rs#L17-L24)

`is_safe_path_segment()` validates artifact names don't contain path separators, `.`, or `..`. Unsafe names produce `Action::Skipped`.

#### Lockfile Drift Detection

[`crates/libaipm/src/lockfile/mod.rs:81-120`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/lockfile/mod.rs#L81-L120)

`validate_matches_manifest()` compares lockfile packages against manifest dependency names. Accumulates issues into `Vec<String>`, joins with `"; "`, returns `Error::Drift`.

#### Store Hash Validation

[`crates/libaipm/src/store/hash.rs:28-36`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/store/hash.rs#L28-L36)

Checks hash string is exactly 128 lowercase hex characters.

#### Registry Checksum Verification

[`crates/libaipm/src/registry/git.rs:178-189`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/registry/git.rs#L178-L189)

Computes SHA-512 and compares against expected. Returns `Error::ChecksumMismatch`.

#### Dry-Run Report Generation

[`crates/libaipm/src/migrate/dry_run.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/dry_run.rs)

Generates markdown reports with sections grouped by artifact kind, summary tables. Closest existing pattern to a lint report output format.

#### User Error Reporting

Both CLIs use the same pattern:
```rust
fn main() -> std::process::ExitCode {
    if let Err(e) = run() {
        let mut stderr = std::io::stderr();
        let _ = writeln!(stderr, "error: {e}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}
```

In-command: `Action::Renamed` is prefixed with `"Warning: "` (string convention, not typed severity). Tracing crate is used for internal logging but not surfaced to users.

#### What Does NOT Exist

- No unified severity levels (warn/error/info as a type)
- No diagnostic type or lint-rule abstraction
- No rule IDs or error codes
- No structured error output (JSON, SARIF)
- No configurable rule sets or rule suppression
- No `--fix` auto-fix infrastructure
- No quality score computation
- No `lint` or `check` subcommand on either CLI

---

### 4. Plugin/Marketplace Directory Structure and Conventions

#### `.ai/` Marketplace Layout

Scaffolded by `workspace_init::scaffold_marketplace()` at [`workspace_init/mod.rs:191-273`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/workspace_init/mod.rs#L191-L273):

```
.ai/
  .gitignore                          # Managed by aipm (marker-delimited sections)
  .claude-plugin/
    marketplace.json                  # Plugin registry for Claude Code discovery
  <plugin-name>/                      # One directory per plugin
    .claude-plugin/
      plugin.json                     # Plugin metadata for Claude Code
    aipm.toml                         # Plugin manifest (optional, opt-in via --manifest)
    skills/<skill-name>/SKILL.md      # Skill definition files
    agents/<agent-name>.md            # Agent definition files
    hooks/hooks.json                  # Hook configuration
    scripts/<script-name>.<ext>       # Utility scripts
    .mcp.json                         # MCP server config
    lsp.json                          # LSP server config
    extensions/<ext-name>/            # Extensions (Copilot-only)
```

#### `marketplace.json` Format

[`.ai/.claude-plugin/marketplace.json`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/.ai/.claude-plugin/marketplace.json)

```json
{
  "name": "local-repo-plugins",
  "owner": { "name": "local" },
  "metadata": { "description": "Local plugins for this repository" },
  "plugins": [
    { "name": "hello-world", "source": "./hello-world", "description": "..." }
  ]
}
```

Managed by `registrar.rs` at [`migrate/registrar.rs:10-51`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/registrar.rs#L10-L51).

#### `plugin.json` Format

```json
{
  "name": "hello-world",
  "description": "A simple hello world skill.",
  "version": "0.1.0"
}
```

Generated by `generate_plugin_json()` at [`workspace_init/mod.rs:294-309`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/workspace_init/mod.rs#L294-L309).

#### `.claude/` Directory vs `.ai/` Marketplace

`.claude/` is Claude Code's **native** configuration directory (migration source). `.ai/` is the **aipm-managed** marketplace directory (migration target).

`.claude/settings.json` bridges the two via `extraKnownMarketplaces`:
```json
{
  "extraKnownMarketplaces": {
    "test-marketplace": {
      "source": { "source": "directory", "path": ".ai" }
    }
  },
  "enabledPlugins": {
    "plugin-name@marketplace-name": true
  }
}
```

Written/merged by Claude adaptor at [`workspace_init/adaptors/claude.rs:19-58`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/workspace_init/adaptors/claude.rs#L19-L58).

#### What Constitutes a Valid Plugin

- **Minimum for Claude Code discovery**: `.claude-plugin/plugin.json` must exist
- **Minimum for aipm manifest**: `[package]` with `name` (valid pattern) and `version` (semver)
- **`aipm.toml` is optional**: opt-in via `--manifest` flag. Claude Code reads `plugin.json` directly

#### YAML Frontmatter Conventions

| Artifact Type | Required Fields | Optional Fields | Parsed By |
|---------------|----------------|-----------------|-----------|
| Skills (`SKILL.md`) | -- | `name`, `description`, `hooks`, `disable-model-invocation` | `skill_common.rs:13-77` |
| Commands (`.md`) | -- | `name`, `description` | `command_detector.rs` |
| Agents (`.md`) | -- | `name`, `description`, `tools`, `model` | `agent_detector.rs` |
| Output Styles (`.md`) | -- | `name`, `description` | `output_style_detector.rs` |

Currently no fields are truly **required** by the parser -- the detectors silently fall back to directory/filename-derived names when frontmatter fields are absent.

#### Plugin Component Types

[`crates/libaipm/src/manifest/types.rs:114-141`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/manifest/types.rs#L114-L141)

Nine component types in `[components]`: `skills`, `commands`, `agents`, `hooks`, `mcp_servers`, `lsp_servers`, `scripts`, `output_styles`, `settings`.

#### Valid Hook Events (22 Total)

From [`research/docs/2026-03-24-claude-code-hooks-settings-styles.md`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/research/docs/2026-03-24-claude-code-hooks-settings-styles.md):

`PreToolUse`, `PostToolUse`, `Notification`, `Stop`, `SubagentStop`, `SubagentStart`, `UserPromptSubmit`, `UserPromptContinue`, `PreCompact`, `PostCompact`, `SessionPause`, `SessionResume`, `ModelResponse`, `ToolError`, `TaskStart`, `TaskComplete`, `McpToolUse`, `McpToolResult`, `McpServerStart`, `McpServerStop`, `AgentStart`, `AgentStop`.

Copilot hook detector normalizes legacy event names (e.g., `"SessionStart"` -> `"sessionStart"`, `"Stop"` -> `"agentStop"`) at [`copilot_hook_detector.rs:89-111`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/copilot_hook_detector.rs#L89-L111).

---

### 5. Configuration File Handling

#### `aipm.toml` Manifest

[`crates/libaipm/src/manifest/types.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/manifest/types.rs)

The `Manifest` struct (line 12) uses `#[serde(deny_unknown_fields)]`. All fields are `Option`, making it flexible for workspace root, member plugin, or hybrid roles.

Key sections:
- **`[package]`** (lines 47-63): `name`, `version`, `description`, `type`, `files`
- **`[workspace]`** (lines 67-76): `members` glob patterns, `plugins_dir`, `dependencies`
- **`[dependencies]`** (lines 81-107): version strings or detailed dependency objects (`DependencySpec` untagged enum)
- **`[components]`** (lines 114-141): nine component path lists
- **`[environment]`** (lines 144-163): `requires`, `aipm`, `platforms`, `strict`, `variables`, `runtime`
- **`[install]`** (lines 198-202): `allowed_build_scripts`
- **`[catalog]`/`[catalogs]`**: default and named version catalogs
- **`[features]`**: feature definitions
- **`[overrides]`**: dependency overrides (root-level only)

#### Parsing Entry Points

[`crates/libaipm/src/manifest/mod.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/manifest/mod.rs)

- `parse(toml_str)` (line 21): `toml::from_str` directly
- `parse_and_validate(toml_str, base_dir)` (line 33): parse + validate
- `load(manifest_path)` (line 45): read file + parse_and_validate

#### Manifest Generation Paths

Three paths generate `aipm.toml` using `format!()` string templates (NOT serde serialization):

1. `init::generate_manifest()` at [`init.rs:187-203`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/init.rs#L187-L203) -- plugin manifest
2. `workspace_init::generate_workspace_manifest()` at [`workspace_init/mod.rs:166-185`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/workspace_init/mod.rs#L166-L185) -- workspace manifest
3. `installer::manifest_editor` at [`installer/manifest_editor.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/installer/manifest_editor.rs) -- comment-preserving edits via `toml_edit`

#### `aipm.lock` Lockfile

[`crates/libaipm/src/lockfile/types.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/lockfile/types.rs)

`Lockfile` struct: `metadata` (with `lockfile_version: u32` = 1, `generated_by: String`) and `packages: Vec<Package>` (serialized as `[[package]]`). Versioned at `LOCKFILE_VERSION` constant.

#### `.aipm/links.toml` Link State

[`crates/libaipm/src/linker/link_state.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/linker/link_state.rs)

`State` struct with `link: Vec<LinkEntry>`. Operations: `read()`, `write()`, `add()`, `remove()`, `clear_all()`, `list()`.

#### Registry Configuration

[`crates/libaipm/src/registry/config.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/registry/config.rs)

`Config` struct with `registries: BTreeMap<String, RegistryEntry>` and `scopes: BTreeMap<String, String>`. Scope-based routing via `registry_for_package()`. The global config file `~/.aipm/config.toml` is modeled but not yet wired up.

#### Workspace Root Discovery

[`crates/libaipm/src/workspace/mod.rs:32-49`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/workspace/mod.rs#L32-L49)

`find_workspace_root(start_dir)` walks up the directory tree looking for `aipm.toml` with a `[workspace]` section.

#### `plugins_dir` Resolution

[`crates/aipm/src/main.rs:185-195`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/aipm/src/main.rs#L185-L195)

Reads `[workspace].plugins_dir` from `aipm.toml`, falls back to `".ai"`. Fixture `workspace-separate-plugins-dir` shows it can be `"plugins"`.

---

### 6. BDD Feature Specification for Lint

[`tests/features/guardrails/quality.feature`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/tests/features/guardrails/quality.feature)

This is the authoritative specification. **None of it is implemented yet.**

#### Lint Rules Specified

| Rule | Severity | Line |
|------|----------|------|
| SKILL.md missing required field: `description` | warning | 28 |
| SKILL.md exceeds recommended 5000 token limit | warning | 33 |
| Agent definition missing `tools` declaration | warning | 38 |
| Unknown hook event name | error | 43 |
| No issues found (clean pass) | -- | 49 |

#### Additional Features Specified

- **Publish gate** (line 54-58): Rejects packages failing lint; displays lint errors; hints `aipm-pack lint --fix`
- **Auto-fix mode** (line 61-65): `--fix` flag; e.g., truncate name to 64 chars
- **Quality score** (line 67-87): Computed on publish; criteria: has description, has license, has readme, has examples, has env deps
- **Structured error guidance** (lines 16-21): machine-readable error code, human-readable fix suggestion, documentation link

---

## Code References

### Adapter/Detector Architecture
- [`crates/libaipm/src/migrate/detector.rs:9-20`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/detector.rs#L9-L20) -- `Detector` trait definition
- [`crates/libaipm/src/migrate/detector.rs:23-53`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/detector.rs#L23-L53) -- Factory functions and source-type dispatch
- [`crates/libaipm/src/migrate/mod.rs:289-321`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/mod.rs#L289-L321) -- `migrate()` entry point
- [`crates/libaipm/src/migrate/discovery.rs:54-76`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/discovery.rs#L54-L76) -- Recursive directory walking
- [`crates/libaipm/src/fs.rs:25-39`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/fs.rs#L25-L39) -- `Fs` trait
- [`crates/libaipm/src/migrate/skill_common.rs:13-77`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/skill_common.rs#L13-L77) -- Shared frontmatter parsing

### CLI Structure
- [`crates/aipm/src/main.rs:20-141`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/aipm/src/main.rs#L20-L141) -- `Commands` enum
- [`crates/aipm/src/main.rs:564-596`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/aipm/src/main.rs#L564-L596) -- `run()` dispatch
- [`crates/aipm-pack/src/main.rs:15-42`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/aipm-pack/src/main.rs#L15-L42) -- `aipm-pack` Commands enum
- [`crates/aipm/src/wizard.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/aipm/src/wizard.rs) -- Pure wizard logic
- [`crates/aipm/src/wizard_tty.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/aipm/src/wizard_tty.rs) -- TTY bridge

### Validation Patterns
- [`crates/libaipm/src/manifest/validate.rs:76-197`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/manifest/validate.rs#L76-L197) -- `validate()` with error accumulation
- [`crates/libaipm/src/manifest/error.rs:10-84`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/manifest/error.rs#L10-L84) -- Manifest error types
- [`crates/libaipm/src/linker/security.rs:20-26`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/linker/security.rs#L20-L26) -- `ScriptVerdict` pattern
- [`crates/libaipm/src/lockfile/mod.rs:81-120`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/lockfile/mod.rs#L81-L120) -- Lockfile drift detection

### Plugin/Marketplace Structure
- [`crates/libaipm/src/manifest/types.rs:12-237`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/manifest/types.rs#L12-L237) -- Full manifest schema
- [`crates/libaipm/src/workspace_init/mod.rs:191-273`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/workspace_init/mod.rs#L191-L273) -- Marketplace scaffolding
- [`crates/libaipm/src/workspace_init/adaptors/claude.rs:19-58`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/workspace_init/adaptors/claude.rs#L19-L58) -- Claude settings integration
- [`crates/libaipm/src/migrate/registrar.rs:10-51`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/migrate/registrar.rs#L10-L51) -- marketplace.json registration

### Configuration Files
- [`crates/libaipm/src/manifest/mod.rs:21-49`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/manifest/mod.rs#L21-L49) -- Parse, validate, load entry points
- [`crates/libaipm/src/lockfile/types.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/lockfile/types.rs) -- Lockfile schema
- [`crates/libaipm/src/linker/link_state.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/linker/link_state.rs) -- Link state schema
- [`crates/libaipm/src/registry/config.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/registry/config.rs) -- Registry config schema
- [`crates/libaipm/src/workspace/mod.rs:32-49`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/workspace/mod.rs#L32-L49) -- Workspace root discovery
- [`crates/libaipm/src/installer/manifest_editor.rs`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/installer/manifest_editor.rs) -- Comment-preserving TOML editing

### BDD Specification
- [`tests/features/guardrails/quality.feature`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/tests/features/guardrails/quality.feature) -- Lint scenarios and quality scoring

---

## Architecture Documentation

### Key Patterns Summary

| Pattern | Location | Mechanism |
|---------|----------|-----------|
| Strategy pattern (detectors) | `migrate/detector.rs` | `Detector` trait with factory functions |
| Source-type dispatch | `migrate/detector.rs:47-53` | `detectors_for_source()` match |
| Filesystem abstraction | `fs.rs:25-39` | `Fs` trait, `Send + Sync` |
| Error accumulation | `manifest/validate.rs:77-110` | `Vec<Error>` -> `Error::Multiple` |
| Binary verdict | `linker/security.rs:20-26` | `ScriptVerdict::Allowed / Blocked` |
| Action-based diagnostics | `migrate/mod.rs:133-187` | `Action::Skipped`, `Action::Renamed` |
| CLI subcommand dispatch | `aipm/src/main.rs:564-596` | clap derive + match on Commands enum |
| Parallel execution | `migrate/mod.rs` | `rayon::par_iter()` for detect and emit |
| Recursive discovery | `migrate/discovery.rs` | `ignore` crate with gitignore awareness |
| Config parsing | `manifest/types.rs` | serde + toml + `deny_unknown_fields` |
| Comment-preserving edits | `installer/manifest_editor.rs` | `toml_edit::DocumentMut` |

### `libaipm` Module Map

[`crates/libaipm/src/lib.rs:7-19`](https://github.com/TheLarkInn/aipm/blob/1b8483daae7b50608a93a114404330d1e235d222/crates/libaipm/src/lib.rs#L7-L19)

| Module | Used By |
|--------|---------|
| `workspace_init` | `Init` |
| `installer::pipeline` | `Install`, `Update` |
| `linker::*` | `Link`, `Unlink` |
| `lockfile` | `List` |
| `migrate`, `migrate::cleanup` | `Migrate` |
| `manifest` | `Install`, `Link` |
| `workspace` | `Install` |
| `registry` | `Install`, `Update` |
| `fs` | `Init`, `Migrate` |

A new `lint` module would slot alongside `migrate` in `libaipm`.

---

## Historical Context (from research/)

### Migrate/Adapter Architecture
- [`research/docs/2026-03-23-aipm-migrate-command.md`](research/docs/2026-03-23-aipm-migrate-command.md) -- Original migrate command research; documents Detector trait, factory pattern, two-phase pipeline
- [`research/docs/2026-03-23-recursive-claude-discovery-parallel-migrate.md`](research/docs/2026-03-23-recursive-claude-discovery-parallel-migrate.md) -- Recursive discovery and parallel detect+emit design
- [`research/docs/2026-03-28-copilot-cli-migrate-adapter.md`](research/docs/2026-03-28-copilot-cli-migrate-adapter.md) -- Copilot adapter implementation; documents separate detectors per tool, shared extraction logic, parameterized discovery
- [`research/docs/2026-03-19-init-tool-adaptor-refactor.md`](research/docs/2026-03-19-init-tool-adaptor-refactor.md) -- Init `ToolAdaptor` trait (separate from `Detector`); documents composable adaptor list, submodule pattern

### Plugin/Marketplace Structure
- [`research/docs/2026-03-16-aipm-init-workspace-marketplace.md`](research/docs/2026-03-16-aipm-init-workspace-marketplace.md) -- Workspace and marketplace initialization
- [`research/docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md`](research/docs/2026-03-24-aipm-toml-generation-in-init-and-migrate.md) -- Five `aipm.toml` generation paths; inconsistent validation; round-trip validation exists but not applied everywhere
- [`research/docs/2026-03-24-claude-code-hooks-settings-styles.md`](research/docs/2026-03-24-claude-code-hooks-settings-styles.md) -- 22 hook event types, 4 handler types, settings scoping, output style format
- [`research/docs/2026-03-24-claude-code-mcp-lsp-config.md`](research/docs/2026-03-24-claude-code-mcp-lsp-config.md) -- MCP and LSP configuration format
- [`research/docs/2026-03-16-claude-code-defaults.md`](research/docs/2026-03-16-claude-code-defaults.md) -- Claude Code directory structure
- [`research/docs/2026-03-28-copilot-cli-source-code-analysis.md`](research/docs/2026-03-28-copilot-cli-source-code-analysis.md) -- Copilot CLI source analysis

### Prior Lint Research
- [`research/tickets/2026-03-28-110-aipm-lint.md`](research/tickets/2026-03-28-110-aipm-lint.md) -- Earlier research on this same issue; documents Detector-to-LintRule mapping, BDD-specified rules, open design questions

---

## Related Research

- [`research/tickets/2026-03-28-110-aipm-lint.md`](research/tickets/2026-03-28-110-aipm-lint.md) -- Direct predecessor to this document
- [`research/docs/2026-03-09-aipm-cucumber-feature-spec.md`](research/docs/2026-03-09-aipm-cucumber-feature-spec.md) -- BDD feature specification
- [`research/docs/2026-03-19-test-inventory-and-gating-strategy.md`](research/docs/2026-03-19-test-inventory-and-gating-strategy.md) -- Test strategy applicable to lint
- [`research/docs/2026-03-09-cargo-core-principles.md`](research/docs/2026-03-09-cargo-core-principles.md) -- Cargo architecture (clippy severity model reference)

---

## Open Questions

1. **Which CLI gets the lint command?** The BDD feature specifies `aipm-pack lint`, but issue #110 says `aipm lint`. These could be separate commands, both CLIs could get it, or the issue title may be shorthand.

2. **Lint target scope**: Does lint scan `.claude/`/`.github/` source directories (checking for misplaced plugin features), or `.ai/` marketplace plugins (validating plugin quality), or both?

3. **Ignore pattern format**: Issue #110 mentions "supports ignore patterns" for the rule checking plugin features aren't inside `.claude` subfolders. What format? Glob patterns? A `.lintignore` file? Inline comments?

4. **Shared frontmatter parser**: Six detectors each implement line-by-line frontmatter parsing. Should a shared YAML frontmatter parser be extracted for lint rules to reuse?

5. **Severity model**: The BDD feature uses "warning" and "error". The codebase has no formal severity enum. The `ScriptVerdict` (binary) and `Error::Multiple` (accumulation) patterns exist as starting points. Should there be an `info` level?

6. **Rule ID system**: The BDD feature mentions "machine-readable error code" (line 19). No rule ID system exists. Format options: `AIPM001`, `skill/missing-description`, clippy-style `aipm::missing_description`, etc.

7. **Lint configuration file design**: Issue #110 mentions "Design for lint file." No lint configuration format exists yet. Should it be a section in `aipm.toml` (e.g., `[lint]`) or a separate file?

8. **LSP integration**: Issue #110 mentions "LSP for VS Code Usage." This implies lint diagnostics should be surfaceable via Language Server Protocol, which affects the diagnostic data model.
