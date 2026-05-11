# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.25.0] - 2026-05-11

### Features

- **`aipm init` is now idempotent** ([#850](https://github.com/TheLarkInn/aipm/pull/850), [#861](https://github.com/TheLarkInn/aipm/pull/861)) — re-running `aipm init` in an already-initialized directory no longer fails. Pre-existing `aipm.toml`, `.ai/` marketplaces, and marketplace manifests are detected and reused; only missing artifacts are created. Stdout reports what was found vs. created (`Using existing aipm.toml in <dir>`, `Found existing <Engine> marketplace manifest at <path>`, etc.). **Breaking change:** `aipm init` now exits zero when all requested artifacts already exist (previously exited non-zero); scripts using `aipm init || …` to detect prior initialization should inspect stdout for `Using existing` messages instead.

### Documentation

- Add `.github/copilot-instructions.md` to README `aipm lint` and `aipm lsp` file-pattern lists ([#835](https://github.com/TheLarkInn/aipm/pull/835)).
- Fix Copilot skill layout descriptions in README and rule count in `configuring-lint.md` ([#836](https://github.com/TheLarkInn/aipm/pull/836)).
- Fix `libaipm-engine-spec` helper function parameter names in README (`path` → `name` for `engine_for_root_dir` and `marketplace_host_for_root_dir`) ([#846](https://github.com/TheLarkInn/aipm/pull/846)).

## [0.24.2] - 2026-05-07

### Documentation

- Added `libaipm-engine-spec` crate reference to README — documents all public types (`Engine`, `EngineSet`), constants (`ENGINES`, `VALID_TOOLS`, `TOOL_COMPATIBILITY`, `FEATURES_BY_ENGINE`, `HOOK_EVENTS_BY_ENGINE`), path helpers, and the `valid_tool_name_check` function ([#809](https://github.com/TheLarkInn/aipm/pull/809)).
- Fixed `VALID_TOOLS` type and description in the `libaipm-engine-spec` reference table ([#815](https://github.com/TheLarkInn/aipm/pull/815)).

## [0.24.1] - 2026-05-06

### Security / Bug Fixes

- **Address #793 — ADO log-command injection, lint path containment, NuGet hardening** ([#804](https://github.com/TheLarkInn/aipm/pull/804)):
  - `ci-azure` reporter now escapes the `##[group]` header line that previously interpolated raw file paths, preventing PR-author-controlled paths with `\r`/`\n` from injecting Azure DevOps logging commands.
  - Lint rules (`marketplace/source-resolve`, `marketplace/plugin-field-mismatch`, `valid-tool-name`) now apply `..`/absolute-path containment checks before any filesystem access; paths that escape the `.ai/` root are reported as errors or silently skipped — see [`docs/guides/source-security.md`](docs/guides/source-security.md) for details.
  - `valid-tool-name` caps its parent-walk at `lint::Options::dir` to prevent escaping the project root.
  - NuGet publish workflow: removes long-lived `NUGET_API_KEY` fallback, binds `workflow_dispatch` to a protected environment with required reviewers, adds SLSA v1 build provenance via `actions/attest-build-provenance`.

## [0.24.0] - 2026-05-05

### Features

- **`libaipm-engine-spec` crate — engine API schema source-of-truth** ([#771](https://github.com/TheLarkInn/aipm/pull/771)) — new `crates/libaipm-engine-spec/` crate whose hand-written Rust types are the canonical shape for the engine API schema. `schemars` derives `schemas/engine-api.schema.json`; `build.rs` validates `data/engine-api-schema.json` against that schema on every build and emits typed `&'static` const tables (`ENGINES`, `VALID_TOOLS`, `TOOL_COMPATIBILITY`, `FEATURES_BY_ENGINE`, `HOOK_EVENTS_BY_ENGINE`). All consumer subsystems collapse onto generated tables, eliminating silent drift between binary reality and Rust constants. See [`specs/2026-05-04-engine-api-schema-source-of-truth.md`](specs/2026-05-04-engine-api-schema-source-of-truth.md).
- **`valid-tool-name` lint rule** (rule 19) — warns or errors when a tool in an agent, skill, or hook `tools` frontmatter field is exclusive to an AI engine not declared in `aipm.toml`. Severity escalates from `warning` (no engines declared) to `error` (declared engines don't support the tool). Powered by `TOOL_COMPATIBILITY` from `libaipm-engine-spec`. See [`docs/rules/valid-tool-name.md`](docs/rules/valid-tool-name.md).

## [0.23.1] - 2026-05-04

### Removed

- **`aipm lint` no longer classifies `claude-instructions.md`, `agents-instructions.md`, or `gemini-instructions.md` as instruction files** — engine-documentation verification (Anthropic Claude Code, Google Gemini CLI, AGENTS.md spec) confirmed no engine reads files with these names. See [`specs/2026-05-02-engine-instructions-md-pattern-removal.md`](specs/2026-05-02-engine-instructions-md-pattern-removal.md). The `copilot-instructions.md` filename **is preserved** because GitHub Copilot reads it at `.github/copilot-instructions.md`. Files matched by `INSTRUCTION_FILENAMES` (`CLAUDE.md`, `AGENTS.md`, `GEMINI.md`, `COPILOT.md`, `INSTRUCTIONS.md`, `copilot-instructions.md`) and the `*.instructions.md` suffix continue to classify normally.

## [0.23.0] - 2026-05-01

### Fixed

- **`aipm migrate` and `aipm lint` now detect skills under `.github/copilot/skills/<name>/SKILL.md`** — closes issue [#725](https://github.com/TheLarkInn/aipm/issues/725). The customer's nested layout (where `.github/copilot/` contains a `skills/` subdirectory) was previously invisible to the migrate detector. The unified discovery pipeline now finds skills at all three Copilot layouts: `.github/skills/<name>/`, `.github/copilot/<name>/`, and `.github/copilot/skills/<name>/`.

### Added

- **`aipm migrate` and `aipm lint` print a scan summary by default** — a single line on stderr describing what the discovery walker matched (`"Scanned N directories in [.github, .claude]; matched 3 skills, 1 instruction"`). Suppressed via `--no-summary` or when `--log-format=json` is set.

### Changed

- **Unified discovery is now unconditionally on** — the previous `AIPM_UNIFIED_DISCOVERY` opt-in env var has been removed. `aipm migrate` and `aipm lint` always go through the new walker + classifier + adapters pipeline. **Breaking change** (alpha): callers that set `AIPM_UNIFIED_DISCOVERY=0` to pin legacy behavior will silently get the unified path. The project is in alpha and breaking changes are accepted.

### Internal / Infrastructure

- **Unified discovery module** — `crates/libaipm/src/discovery/` containing walker + classifier shared by both `migrate` and `lint`, plus the migrate adapter pipeline at `crates/libaipm/src/migrate/adapters/`. Replaces the asymmetric two-pipeline architecture documented in `research/docs/2026-05-01-github-copilot-skills-migrate-lint-silent-failure.md`.
- **Hybrid migrate orchestrator** — `migrate::unified::run` now invokes the unified adapters for kinds with `FeatureKind` variants (Skill, Agent, Copilot Hook) and falls back to the legacy detectors per source dir for the deferred kinds (Claude embedded `settings.json` hook, MCP, Extension, LSP, Command, OutputStyle). Package-scoped sources merge all artifacts (adapter + legacy) into a single plugin named after the package.
- **Retired** `discovery::UNIFIED_DISCOVERY_ENV`, `discovery::unified_enabled()`, `migrate::unified::unified_enabled()`, the `discovery::legacy_compat` adapter module, the `discovery::test_env` test helper, the legacy `migrate_recursive` / `migrate_single_source` paths, and `discovery_legacy::discover_features` (only the source-dir enumeration helpers remain).

## [0.22.5] - 2026-04-30

### Features

- **`reverse-binary-analysis` agentic workflow** — new weekly workflow that downloads each configured AI engine CLI (`claude`, `copilot-cli`, …), performs parallel LLM-assisted reverse analysis of bundled source, and maintains `research/engine-api-schema.json` and `research/engine-api-changelog.md`. Opens a PR when the schema changes. See `.github/workflows/reverse-binary-analysis.md`.

### Internal / Infrastructure

- **`copilot-setup-steps` Cargo pre-fetch** — pre-fetches all workspace Cargo dependencies during environment setup to prevent network-sandbox build failures inside agentic workflow containers.

## [0.22.4] - 2026-04-24

### Features

- **NuGet publishing pipeline** — `aipm` is now published to [nuget.org](https://www.nuget.org/packages/aipm) as a multi-RID native package (`win-x64`, `linux-x64`, `osx-x64`, `osx-arm64`), enabling Azure DevOps pipelines to install via `dotnet restore` without `curl | sh`. See [docs/guides/install-nuget.md](docs/guides/install-nuget.md).
- **Azure DevOps lint reporter enrichment** — `ci-azure` reporter now emits richer `##vso[task.logissue]` lines that include help text and help URL in the logissue message body, collapsible per-file `##[group]` sections, and a `SucceededWithIssues` completion signal on warnings-only runs.

### Documentation

- Add `docs/guides/install-nuget.md` — Azure DevOps NuGet installation guide with caching and lint integration.
- Add `aipm make plugin` guide and update command table in README.
- Add `aipm update` guide and lockfile semantics reference.
- Add `aipm init` workspace initialization guide.
- Add VS Code extension guide and `aipm lsp` command reference.
- Add `instructions/oversized` rule documentation and `18-rule` lint coverage notes.

### `aipm make plugin` (v0.22.0+)

- **`aipm make plugin`** — new scaffolding command that creates plugin directories inside an existing `.ai/` marketplace, writes `.claude-plugin/plugin.json`, and registers the plugin in `marketplace.json`. Supports `--engine claude|copilot|both|lsp|extension` and `--yes` for non-interactive use.

### `instructions/oversized` lint rule (v0.20.0+)

- New rule `instructions/oversized` — warns when instruction files (`CLAUDE.md`, `AGENTS.md`, `COPILOT.md`, `GEMINI.md`, `INSTRUCTIONS.md`, `*.instructions.md`) exceed the configured line or character limit. Configurable via `resolve-imports`, `lines`, and `characters` options in `aipm.toml`.

## [0.19.7] - 2026-04-11

### Features

- **`aipm` consumer CLI** — `init`, `install`, `update`, `uninstall`, `link`, `unlink`, `list`, `lint`, `migrate`, `lsp` commands
- **`aipm-pack` author CLI** — `init` command for scaffolding new plugin packages
- **`aipm lint`** — unified, gitignore-aware quality linter with 17 rules across `skill/`, `agent/`, `hook/`, `plugin/`, `marketplace/`, and `source/` categories; supports `human`, `json`, `ci-github`, and `ci-azure` reporters
- **`aipm migrate`** — recursive discovery and migration of Claude Code (`.claude/`) and Copilot CLI (`.github/`) configurations into structured `.ai/` marketplace plugins; supports dry-run, destructive cleanup, and all artifact types (skills, agents, MCP servers, hooks, commands, output styles, extensions, LSP servers)
- **`aipm lsp`** — Language Server Protocol server powering real-time lint diagnostics, `aipm.toml` completions, and hover documentation
- **`vscode-aipm` extension** — VS Code integration via LSP; inline diagnostics, rule-ID completions, hover docs, and TOML schema validation for `aipm.toml`
- **Multi-source install** — install plugins from registry, `github:`, `git:`, `local:`, and `market:`/`marketplace:` spec formats
- **Global plugin registry** — `~/.aipm/` store with engine scoping and name-conflict detection
- **Download cache** — 5 cache policies with per-entry TTL
- **Source security** — configurable allowlist with path-traversal protection
- **Workspace support** — `[workspace]` manifest with member glob expansion and shared lints config
- **Engine & platform compatibility** — two-tier validation against `aipm.toml` `engines` field and marker files
- **`aipm.toml` JSON Schema** — available at `schemas/aipm.toml.schema.json` and via SchemaStore

