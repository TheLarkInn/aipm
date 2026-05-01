---
date: 2026-05-01 15:17:09 UTC
researcher: Sean Larkin
git_commit: e0f755bfdf0a63efcd10d5d5b436134c1fe58e85
branch: main
repository: aipm
topic: "Why aipm migrate and aipm lint silently do nothing on a customer's `.github/copilot/` skills folder"
tags: [research, codebase, migrate, lint, copilot, discovery, silent-failure, engine-paths]
status: complete
last_updated: 2026-05-01
last_updated_by: Sean Larkin
---

# Research

## Research Question

A customer using `aipm migrate` and `aipm lint` showed me that their `.github/copilot/` folder contained skills, but both `migrate` and `lint` weren't working at all. Document:

(a) Canonical engine→path mappings the codebase recognizes for GitHub Copilot.
(b) How `migrate` discovers source artifacts and where it silently misses `.github/copilot/skills/`.
(c) How `lint` discovers and validates artifacts and where it silently skips a non-canonical path.
(d) Any registry/config that maps engine names to filesystem paths.
(e) The user-facing output (or lack of it) when discovery yields nothing.

## Summary

`aipm migrate` and `aipm lint` use **two completely different and asymmetric** discovery mechanisms, both with hard-coded path expectations and both with silent-skip behavior on unrecognized layouts.

- **`migrate`** scans only two engine roots (`.claude` and `.github`) and dispatches a fixed list of detectors per root (`crates/libaipm/src/migrate/mod.rs:399`, `crates/libaipm/src/migrate/detector.rs:47-53`). The Copilot skill detector inspects exactly the children of `.github/skills/` and `.github/copilot/` for `<entry>/SKILL.md` — one level deep, no recursion (`crates/libaipm/src/migrate/copilot_skill_detector.rs:27-69`).
- **`lint`** does a single recursive `ignore::Walk` of the project root and classifies each file via `classify_feature_kind` (`crates/libaipm/src/discovery.rs:243-297`). For a `SKILL.md` it requires the parent **or** grandparent directory name to literally equal `"skills"`. Anything else is silently dropped at `crates/libaipm/src/discovery.rs:339`.
- The two pipelines do **not** share their discovery layer. `migrate` does not call `discover_features`; `lint` does not call any `*_detector`.

The asymmetry produces a layout-dependent silent failure mode for `.github/copilot/`-style content. Two plausible customer layouts both fail, just on different commands:

| Customer layout | `migrate` finds it? | `lint` finds it? |
|---|---|---|
| `.github/copilot/<skill>/SKILL.md` | **Yes** — matched by `copilot_skill_detector` | **No** — `parent="<skill>"`, `grandparent="copilot"`, neither `=="skills"` |
| `.github/copilot/skills/<skill>/SKILL.md` | **No** — detector only joins `skills`/`copilot` *once* onto `.github`; expected `SKILL.md` is at `.github/copilot/skills/SKILL.md` (does not exist) | **Yes** — `grandparent=="skills"` matches |
| `.github/copilot/<group>/<skill>/SKILL.md` | **No** — detector only goes one level deep | **No** — neither parent nor grandparent is `"skills"` |

Both commands are also engineered to be quiet on the "found nothing" case:
- `cmd_migrate` returns `Ok(())` with **zero stdout output** when no artifacts migrate (`crates/aipm/src/main.rs:957`).
- `cmd_lint`'s `human` reporter prints `"no issues found"`, the `ci-github` reporter writes nothing, and `ci-azure` early-returns with no output (`crates/libaipm/src/lint/reporter.rs:106-133, 309-380`).

The repository already has tickets diagnosing variants of this problem: `#187` (lint flat vs migrate recursive), `#208` (lint silently producing zero diagnostics for `.github/`-nested content), and `#123` (migrate silently dropping unrecognized files). All three are precedents for the customer's complaint.

## Detailed Findings

### Engine→Path Registry (what is canonical?)

There is no single registry. Engine identity and engine paths live in several discrete places, and `lint` and `migrate` consult different ones.

**Engine identity**:
- `crates/libaipm/src/engine.rs:14` — `pub enum Engine { Claude, Copilot }`.
- `crates/libaipm/src/engine.rs:51` — `all_names() -> &["claude", "copilot"]`.
- `crates/libaipm/src/engine.rs:23-32` — `marker_files()` map (`.claude-plugin/plugin.json`, `plugin.json`, `.github/plugin/plugin.json`).
- `crates/libaipm/src/engine.rs:36-40` — `marketplace_manifest_path()` map (`.github/plugin/marketplace.toml` for Copilot — note this is a *plugin packaging* path, not a *skills* path).

**Engine roots used by `migrate`**:
- `crates/libaipm/src/migrate/mod.rs:399` — recursive walker pattern list: `&[".claude", ".github"]`.
- `crates/libaipm/src/migrate/detector.rs:47-53` — `detectors_for_source(source_type)` matches **only** the literal strings `".claude"` and `".github"`; everything else returns an empty `Vec`.

**Engine roots recognized by `lint`** (only as diagnostic-tag labels, not as discovery seeds):
- `crates/libaipm/src/discovery.rs:192-222` — `classify_source_context` matches only `.ai`, `.claude`, `.github`.
- `crates/libaipm/src/lint/rules/scan.rs:31-43` — `source_type_from_path` matches the same three names.
- `crates/aipm/src/main.rs:720` — `SUPPORTED_SOURCES: &[".claude", ".github", ".ai"]` validates `--source`. A user typing `aipm lint --source .github/copilot` is rejected outright.

**Make-side engine→feature matrix** (orthogonal — used by `aipm make`, not by migrate/lint):
- `crates/libaipm/src/make/engine_features.rs:82-101` — `CLAUDE_FEATURES`, `COPILOT_FEATURES`, `features_for_engine()` matching `"claude" | "copilot" | "both"`.

**`.github/copilot/` is referenced as a path in only two production code sites**:
- `crates/libaipm/src/migrate/copilot_skill_detector.rs:14, 26-27` — the only place that joins literal `"copilot"` onto `.github` for skill discovery.
- `specs/2026-04-14-aipm-make-plugin-command.md:66, 814, 855` — explicit non-goal NG4: "no `.github/copilot/settings.json` generation" by `aipm make`.

The repo's own `.github/copilot/settings.json` exists for the GitHub Copilot coding agent setup, but this folder contains no skills.

### `aipm migrate` Discovery — Silent Drop on Unrecognized Layouts

#### Entry points and call graph
- CLI subcommand: `crates/aipm/src/main.rs:185-213` (`Commands::Migrate { dry_run, destructive, source, max_depth, manifest, dir }`).
- CLI handler: `crates/aipm/src/main.rs:889-992` (`cmd_migrate`).
- Library entry: `crates/libaipm/src/migrate/mod.rs:260-298` (`pub fn migrate`).
- Recursive path: `crates/libaipm/src/migrate/mod.rs:387-479` (`migrate_recursive`).
- Single-source path: `crates/libaipm/src/migrate/mod.rs:301-384` (`migrate_single_source`).

#### The hard-coded engine roots
`crates/libaipm/src/migrate/mod.rs:398-399`:
```rust
let discovered =
    crate::discovery::discover_source_dirs(dir, &[".claude", ".github"], max_depth)?;
```
There are exactly two engine roots. Anything outside them is invisible to migrate.

`discover_source_dirs` (`crates/libaipm/src/discovery.rs:104-175`) walks the tree gitignore-aware, and at line 142 keeps a directory only if its file name **exactly equals** one of the patterns. It also explicitly excludes `.ai` (lines 120-126).

#### Per-engine detector dispatch
`crates/libaipm/src/migrate/detector.rs:47-53`:
```rust
match source_type {
    ".claude" => claude_detectors(),
    ".github" => copilot_detectors(),
    _ => Vec::new(),
}
```

The six Copilot detectors (`crates/libaipm/src/migrate/detector.rs:35-44`) and the directories they scan, all relative to `source_dir = .github`:

| Detector | Path scanned | Reference |
|---|---|---|
| `CopilotSkillDetector` | `<source>/skills/<entry>/SKILL.md` AND `<source>/copilot/<entry>/SKILL.md` | `crates/libaipm/src/migrate/copilot_skill_detector.rs:27-69` |
| `CopilotAgentDetector` | `<source>/agents/*.md` and `*.agent.md` | `crates/libaipm/src/migrate/copilot_agent_detector.rs:22-58` |
| `CopilotMcpDetector` | `<project_root>/.copilot/mcp-config.json` | `crates/libaipm/src/migrate/copilot_mcp_detector.rs:27` |
| `CopilotHookDetector` | `<source>/hooks.json` or `<source>/hooks/hooks.json` | `crates/libaipm/src/migrate/copilot_hook_detector.rs:20-26` |
| `CopilotExtensionDetector` | `<source>/extensions/<entry>/...` | `crates/libaipm/src/migrate/copilot_extension_detector.rs:20` |
| `CopilotLspDetector` | `<source>/lsp.json` | `crates/libaipm/src/migrate/copilot_lsp_detector.rs:22` |

#### The exact one-level-deep limit in `CopilotSkillDetector`
`crates/libaipm/src/migrate/copilot_skill_detector.rs:23-69` is the only code path that knows about `.github/copilot/`. Reading it line by line:

```rust
for subdir in &["skills", "copilot"] {           // line 27
    let skills_dir = source_dir.join(subdir);    // .github/skills/ or .github/copilot/
    if !fs.exists(&skills_dir) { continue; }     // line 29-31
    let entries = fs.read_dir(&skills_dir)?;     // line 33
    for entry in entries {                       // line 35
        if !entry.is_dir { continue; }           // line 36-38
        let entry_dir = skills_dir.join(&entry.name);     // line 40
        let skill_md = entry_dir.join("SKILL.md");        // line 41
        if !fs.exists(&skill_md) { continue; }            // line 43-45
        ...                                      // build Artifact
    }
}
```

Concrete consequence:
- For `source_dir = .github` and the customer layout `.github/copilot/skills/<skill>/SKILL.md`, the iteration is `subdir = "copilot"` → `skills_dir = .github/copilot` → entry is the directory named `skills` → `entry_dir = .github/copilot/skills` → `skill_md = .github/copilot/skills/SKILL.md` → does not exist → `continue`. The actual `SKILL.md` is one level deeper than the detector looks. **Silent skip.**
- For the canonical layout `.github/copilot/<skill>/SKILL.md` it works, because `entry_dir = .github/copilot/<skill>` and `<skill>/SKILL.md` exists.

There is no second pass. There is no recursion under `<skills_dir>/<entry>/`. The `crates/libaipm/tests/bdd.rs:756-766` BDD step fixture writes exactly the working layout (`.github/copilot/<name>/SKILL.md`), so this layout-shape assumption is also baked into the test suite.

#### What the user sees when `migrate` finds zero artifacts
`crates/libaipm/src/migrate/mod.rs:404-406` — when `discovered.is_empty()` (no `.claude`/`.github` anywhere) it returns `Ok(Outcome { actions: Vec::new() })` with no warning.

`crates/libaipm/src/migrate/mod.rs:412-460` — when `.github` IS discovered but no detector matches, `all_artifacts` is empty, the reconciler still produces `other_files` for every file in `.github/`, but at line 444 the `plans.first_mut()` guard returns `None` (because `plans` is built from the empty `all_artifacts`), so `other_files` is dropped on the floor. After `plugin_plans.retain(|p| !p.artifacts.is_empty())` (line 459), the plan list is empty. `emit_and_register` is called with an empty list and returns `Outcome { actions: vec![] }`.

`crates/aipm/src/main.rs:957`:
```rust
if dry_run || !result.has_migrated_artifacts() { return Ok(()); }
```
With zero `PluginCreated` actions, `cmd_migrate` returns `Ok(())` after the action-printing loop processed nothing. **Zero stdout, zero stderr, exit 0.** The user sees no indication that anything was checked or skipped. `tracing::debug!` calls fire (`crates/libaipm/src/migrate/mod.rs:400-403, 460`) but only surface with `RUST_LOG=debug`.

### `aipm lint` Discovery — Silent Drop in `classify_feature_kind`

#### Entry points and call graph
- CLI subcommand: `crates/aipm/src/main.rs:158-183` (`Commands::Lint`).
- CLI handler: `crates/aipm/src/main.rs:708-788` (`cmd_lint`).
- Library entry: `crates/libaipm/src/lint/mod.rs:120-167` (`pub fn lint`).
- Discovery: `crates/libaipm/src/discovery.rs:299-369` (`discover_features`).

#### There is no manifest-driven engine registry on the lint side
`crates/libaipm/src/lint/mod.rs:125`:
```rust
let features = crate::discovery::discover_features(&opts.dir, opts.max_depth)?;
```
Lint never reads the engines list from `aipm.toml`, never consults a per-engine root list, and never calls `discover_source_dirs`. Discovery is a single recursive `ignore::Walk` of `project_root`, with a fixed `SKIP_DIRS` list (`crates/libaipm/src/discovery.rs:178-179`):
```rust
&["node_modules", "target", ".git", "vendor", "__pycache__", "dist", "build"]
```

#### The classifier — the silent-drop point
`crates/libaipm/src/discovery.rs:243-297` (`classify_feature_kind`) inspects only `file_name`, `parent_name`, and `grandparent_name`. For `SKILL.md`:
```rust
if file_name == "SKILL.md" {
    if parent_name == "skills" || grandparent_name == "skills" {
        return Some(FeatureKind::Skill);
    }
}
```

Files that don't match return `None`, and the walk loop drops them at `crates/libaipm/src/discovery.rs:339`:
```rust
let Some(kind) = classify_feature_kind(file_path) else { continue };
```

Concrete consequences for `.github/copilot/`-style layouts:
- `.github/copilot/<skill>/SKILL.md` — `parent_name="<skill>"`, `grandparent_name="copilot"`. Neither equals `"skills"`. **Silent drop.**
- `.github/copilot/skills/<skill>/SKILL.md` — `parent_name="<skill>"`, `grandparent_name="skills"`. The grandparent branch matches → classified as `Skill`. Found.
- `.github/copilot/<group>/<skill>/SKILL.md` (deeper nesting) — `parent_name="<skill>"`, `grandparent_name="<group>"`. **Silent drop.**

The classifier never inspects ancestors above the grandparent level (except for `plugin.json`, which checks `great_grandparent == ".ai"`). Source-context labels (`crates/libaipm/src/discovery.rs:192-222`) are only assigned **after** classification succeeds, so `.github` membership is irrelevant to whether discovery picks the file up.

#### Quality-rule fan-out
After discovery, `crates/libaipm/src/lint/mod.rs:151-153` calls `run_rules_for_feature`, which dispatches by `FeatureKind` (`crates/libaipm/src/lint/rules/mod.rs:124-170`). All Copilot-derived rules — including `skill_name_invalid` (Copilot CLI regex, `crates/libaipm/src/lint/rules/skill_name_invalid.rs:13`), `skill_oversized` (Copilot character budget, `crates/libaipm/src/lint/rules/skill_oversized.rs:3,12`), and the hook event lists (`crates/libaipm/src/lint/rules/known_events.rs:40-58`) — are keyed off `FeatureKind`, not engine. They will only run on a file once discovery has accepted it. If classification returns `None`, no rule ever fires.

#### What the user sees when lint finds zero diagnostics
`crates/libaipm/src/lint/reporter.rs`:
- `human` reporter (lines 106-133) and `text` reporter (lines 25-49): write `"no issues found\n"` to stdout (line 116 / 32).
- `json` reporter (lines 241-303): emits `{"diagnostics": [], "sources_scanned": []}`.
- `ci-github` reporter (lines 309-331): the `for d in &outcome.diagnostics` loop never enters; nothing is written.
- `ci-azure` reporter (lines 339-380): `if outcome.diagnostics.is_empty() { return Ok(()) }` early-return at line 341-343.

`crates/aipm/src/main.rs:781-785` returns `Err` only when `outcome.error_count > 0`. Zero diagnostics → exit 0.

So the user's observable output depends on the reporter:
- Default `human` / `text`: `"no issues found"` — looks like a clean pass.
- `ci-github` / `ci-azure`: literal silence.

In both cases the user has no signal that lint walked their `.github/copilot/skills/...` files and dropped them all in classification.

#### Tests do not cover the customer's layout
- `tests/features/manifest/migrate.feature:187-199` — only place that exercises `.github/copilot/`, and only via `aipm migrate`. The fixture (`crates/libaipm/tests/bdd.rs:756-766`) is `.github/copilot/<name>/SKILL.md` — the exact layout the detector expects, with no extra `skills/` segment.
- Lint integration tests in `crates/libaipm/src/lint/mod.rs` use only `.claude/skills/<name>/SKILL.md`, `.github/skills/<name>/SKILL.md`, and `.ai/<plugin>/skills/<name>/SKILL.md` (e.g. lines 464-466, 487, 605-607, 663, 762, 792). No test exercises `.github/copilot/...` with lint.

### Pipeline-Level Silent-Failure Points (the fixed-list loops)

For both commands, every code site that looks at a hard-coded engine root or path component:

**Migrate**:
- `crates/libaipm/src/migrate/mod.rs:399` — `&[".claude", ".github"]`.
- `crates/libaipm/src/migrate/detector.rs:47-53` — `match source_type { ".claude" => ..., ".github" => ..., _ => Vec::new() }`.
- `crates/libaipm/src/migrate/copilot_skill_detector.rs:27` — `for subdir in &["skills", "copilot"]`.
- `crates/libaipm/src/migrate/copilot_agent_detector.rs:23` — `source_dir.join("agents")`.
- `crates/libaipm/src/migrate/copilot_extension_detector.rs:20` — `source_dir.join("extensions")`.
- `crates/libaipm/src/migrate/copilot_hook_detector.rs:20-21` — `hooks.json`, `hooks/hooks.json`.
- `crates/libaipm/src/migrate/copilot_lsp_detector.rs:22` — `source_dir.join("lsp.json")`.
- `crates/libaipm/src/migrate/copilot_mcp_detector.rs:27` — `project_root.join(".copilot").join("mcp-config.json")`.

**Lint**:
- `crates/libaipm/src/discovery.rs:268-295` — the `classify_feature_kind` parent/grandparent name table.
- `crates/libaipm/src/discovery.rs:339` — the `else { continue }` silent-drop point.
- `crates/libaipm/src/discovery.rs:192-222` — `classify_source_context` recognizes only `.ai`, `.claude`, `.github`.
- `crates/libaipm/src/lint/rules/scan.rs:31-43` — `source_type_from_path` mirrors the same three names.
- `crates/aipm/src/main.rs:720` — `SUPPORTED_SOURCES: &[".claude", ".github", ".ai"]`.

### What the Copilot CLI Itself Expects On Disk

Per `research/docs/2026-03-28-copilot-cli-source-code-analysis.md` (decompiled `app.js` from Copilot CLI v1.0.12):
- `.github/skills/` (project-level) and `~/.copilot/skills/` (user-level) for skills.
- `.github/agents/` (project-level) for agents.

And per `research/docs/2026-03-31-cli-binary-frontmatter-hook-analysis.md` — same paths confirmed by binary analysis.

`.github/copilot/<skill>/SKILL.md` is supported by aipm's detector as a non-canonical-but-recognized alternative (`crates/libaipm/src/migrate/copilot_skill_detector.rs:14, 26-27`), but the Copilot CLI itself does not treat `.github/copilot/` as a skills root. Customers who organize skills under `.github/copilot/skills/...` are following neither the Copilot CLI's expectations nor aipm's detector layout — they may be modeling on the `.claude/skills/<name>/SKILL.md` shape and assuming it transfers to a `copilot/`-named subtree.

## Code References

- `crates/aipm/src/main.rs:158-213` — CLI declarations for `Lint` and `Migrate` subcommands and their flags.
- `crates/aipm/src/main.rs:708-788` — `cmd_lint` handler.
- `crates/aipm/src/main.rs:720` — `SUPPORTED_SOURCES` lint `--source` allowlist.
- `crates/aipm/src/main.rs:889-992` — `cmd_migrate` handler.
- `crates/aipm/src/main.rs:957` — silent early-return when `migrate` has zero migrated artifacts.
- `crates/libaipm/src/engine.rs:14-71` — `Engine` enum, `marker_files`, `marketplace_manifest_path`.
- `crates/libaipm/src/discovery.rs:104-175` — `discover_source_dirs` (used by `migrate` only).
- `crates/libaipm/src/discovery.rs:178-179` — `SKIP_DIRS`.
- `crates/libaipm/src/discovery.rs:186` — `INSTRUCTION_FILENAMES`.
- `crates/libaipm/src/discovery.rs:192-222` — `classify_source_context`.
- `crates/libaipm/src/discovery.rs:243-297` — `classify_feature_kind` (lint's silent-drop classifier).
- `crates/libaipm/src/discovery.rs:299-369` — `discover_features` (lint's only discovery routine).
- `crates/libaipm/src/discovery.rs:339` — the `else { continue }` line that drops unclassified files.
- `crates/libaipm/src/lint/mod.rs:120-167` — `lint` library entry.
- `crates/libaipm/src/lint/mod.rs:125` — single call site for `discover_features`.
- `crates/libaipm/src/lint/rules/mod.rs:124-170` — `quality_rules_for_kind` per-FeatureKind dispatch.
- `crates/libaipm/src/lint/rules/scan.rs:31-43` — `source_type_from_path`.
- `crates/libaipm/src/lint/rules/known_events.rs:40-58` — `COPILOT_EVENTS`, `COPILOT_LEGACY_MAP`.
- `crates/libaipm/src/lint/rules/skill_name_invalid.rs:13` — `is_valid_copilot_name` regex.
- `crates/libaipm/src/lint/rules/skill_oversized.rs:3,12` — Copilot character budget threshold.
- `crates/libaipm/src/lint/reporter.rs:106-133` — `human` reporter (`"no issues found"`).
- `crates/libaipm/src/lint/reporter.rs:309-331` — `ci-github` reporter (silent on zero diagnostics).
- `crates/libaipm/src/lint/reporter.rs:339-380` — `ci-azure` reporter (early return on zero diagnostics).
- `crates/libaipm/src/migrate/mod.rs:260-298` — `migrate` library entry.
- `crates/libaipm/src/migrate/mod.rs:387-479` — `migrate_recursive`.
- `crates/libaipm/src/migrate/mod.rs:399` — hard-coded `&[".claude", ".github"]` engine roots.
- `crates/libaipm/src/migrate/mod.rs:412-460` — reconcile and plan-pruning loop where `other_files` is dropped.
- `crates/libaipm/src/migrate/detector.rs:23-53` — `claude_detectors`, `copilot_detectors`, `detectors_for_source`.
- `crates/libaipm/src/migrate/copilot_skill_detector.rs:23-73` — Copilot skill detector with the `["skills", "copilot"]` subdir loop.
- `crates/libaipm/src/migrate/copilot_agent_detector.rs:22-58` — Copilot agent detector.
- `crates/libaipm/src/migrate/copilot_hook_detector.rs:20-26` — Copilot hook detector.
- `crates/libaipm/src/migrate/copilot_extension_detector.rs:20` — Copilot extension detector.
- `crates/libaipm/src/migrate/copilot_lsp_detector.rs:22` — Copilot LSP detector.
- `crates/libaipm/src/migrate/copilot_mcp_detector.rs:27` — Copilot MCP detector (note: `.copilot/`, not `.github/copilot/`).
- `crates/libaipm/src/make/engine_features.rs:82-101` — `make` engine→feature matrix.
- `crates/libaipm/tests/bdd.rs:756-766` — BDD step `given_copilot_skill_exists` writes the canonical `.github/copilot/<name>/SKILL.md` layout.
- `tests/features/manifest/migrate.feature:187-199` — only Copilot scenarios in feature suite (migrate-only).

## Architecture Documentation

**Two parallel discovery worlds**: aipm's migrate and lint pipelines were architected separately and have not converged. Migrate uses a fixed engine-root list + per-root detector dispatch + per-detector hard-coded subdirectory paths. Lint uses a single recursive walk + a parent/grandparent name classifier + per-`FeatureKind` rule dispatch. Neither pipeline reads `[package.engines]` from `aipm.toml` to drive discovery (the field exists in the manifest types — `crates/libaipm/src/manifest/types.rs:64-66` — but only affects packaging output).

**`.github/copilot/` is treated specially in only one place**: the Copilot skill detector accepts both `.github/skills/` and `.github/copilot/` as skill roots, **but only one level deep** — `<root>/<skill>/SKILL.md`. Lint has no analogous code; its classifier looks at the parent and grandparent of the file, and the literal name `"copilot"` is never a recognized parent.

**Silent-skip is a deliberate (or at least consistent) design**: across all detectors, missing-or-unrecognized layouts produce empty output rather than warnings or errors. The `Outcome` types for both commands carry no notion of "discovery scanned but found nothing." The CLI handlers print only success-path actions and rely on reporters to surface diagnostics — when the diagnostic list is empty, output is either a single neutral message or nothing at all.

**Reverse-engineering precedent**: the Copilot CLI itself (per `research/docs/2026-03-28-copilot-cli-source-code-analysis.md`) reads `.github/skills/` and `.github/agents/`. aipm's detector adds `.github/copilot/` as an additional accepted root, presumably to accommodate variant layouts seen in customer projects, but this accommodation never made it into lint and never extended past one directory level in migrate.

## Historical Context (from research/)

Most direct precedents for the customer complaint:

- `research/tickets/2026-04-04-208-lint-recursive-discovery-github.md` — Issue #208: `source/misplaced-features` recursion fails for `.github/`-nested content; lint silently produces zero diagnostics. Same shape of bug as the customer's lint complaint.
- `research/tickets/2026-04-02-187-misplaced-features-recursive-discovery.md` — Issue #187: documents the asymmetry that lint did flat-only checks while migrate did recursive walking.
- `research/tickets/2026-04-01-123-migrate-other-files-handling.md` — Issue #123: explicitly notes "Files that exist in source directories but aren't recognized by any of the 12 detectors are silently dropped." Canonical prior art for "migrate silently does nothing."
- `research/docs/2026-04-07-313-migrate-eisdir-crash.md` and `specs/2026-04-07-fix-migrate-eisdir-crash-and-add-logging.md` — Issue #313 added pipeline `tracing::debug!` logging specifically because users couldn't see why migrate seemed to do nothing.

Copilot CLI ground-truth references:

- `research/docs/2026-03-28-copilot-cli-source-code-analysis.md` — Decomposed Copilot CLI v1.0.12 source. Confirms project-level skills root is `.github/skills/`, not `.github/copilot/`.
- `research/docs/2026-03-31-cli-binary-frontmatter-hook-analysis.md` — Binary analysis of frontmatter, hook events, validation rules.
- `research/docs/2026-03-16-copilot-agent-discovery.md` — Earliest survey of Copilot's `.github/agents/` and `.agent.md` discovery.

Migrate-Copilot-adapter design:

- `research/docs/2026-03-28-copilot-cli-migrate-adapter.md` and `specs/2026-03-28-copilot-cli-migrate-adapter.md` — Origin of `copilot_detectors()` and the `.github` source dispatch. Key in `specs/2026-03-28-copilot-cli-migrate-adapter.md:217`: `"github" => detector::copilot_detectors()`.
- `research/docs/2026-03-23-aipm-migrate-command.md` and `specs/2026-03-23-aipm-migrate-command.md` — Day-1 migrate design; `specs/...:47` anticipates `.github/copilot` as a future scan root requiring "ad-hoc code."

Lint architecture:

- `research/tickets/2026-03-28-110-aipm-lint.md:75` — "Currently returns `Error::UnsupportedSource` for other sources. Issue #110 requires the same adapter pattern so lint can run against `.github/copilot`, `.opencode`, etc." This requirement was identified and never fully implemented.
- `research/docs/2026-03-31-110-aipm-lint-architecture-research.md` — LintRule trait, adapter pattern, source-directory dispatch.
- `specs/2026-03-31-aipm-lint-command.md:141` — Planned `copilot_lint_rules()` to mirror `copilot_detectors()` (per-engine rule sets in spec, but actual implementation keys rules by `FeatureKind`, not engine).

Make-side:

- `specs/2026-04-14-aipm-make-plugin-command.md:66, 814, 855` — Explicit non-goal NG4: `.github/copilot/settings.json` generation deferred.

## Related Research

- `research/docs/2026-04-01-migrate-file-discovery-classification.md` — Documents migrate's three-stage discovery → detection → emission pipeline.
- `research/docs/2026-04-01-migrate-file-movement-paths.md` — Path-rewriting logic from `.claude`/`.github` into `.ai/`.
- `research/docs/2026-04-02-aipm-lint-configuration-research.md` — Lint configuration and severity overrides.
- `research/docs/2026-03-23-recursive-claude-discovery-parallel-migrate.md` — Recursive walker design.
- `specs/2026-04-04-lint-unified-file-discovery.md` — Spec to unify migrate+lint discovery (response to #187/#208).

## Open Questions

1. Which exact directory layout did the customer use? Two plausible shapes (`.github/copilot/<skill>/SKILL.md` vs `.github/copilot/skills/<skill>/SKILL.md`) hit different commands, but the customer reported both `migrate` and `lint` failing — suggesting either a deeper layout (`.github/copilot/<group>/<skill>/SKILL.md`, which fails both) or two distinct issues observed simultaneously.
2. Is the Copilot CLI's own behavior (project-level `.github/skills/` per binary analysis) the canonical layout aipm should align on, or should `.github/copilot/skills/` be promoted to a recognized layout — and if so, in which command(s)?
3. Did `specs/2026-04-04-lint-unified-file-discovery.md`'s unification reach implementation, or is the asymmetry described in #187/#208 still present in live code? The classifier shape in `crates/libaipm/src/discovery.rs:243-297` suggests partial unification at best.
4. The `Outcome` types for both commands have no "scanned-but-empty" channel. Should the CLI surface a "scanned N directories, matched 0 files" diagnostic by default, irrespective of reporter?
