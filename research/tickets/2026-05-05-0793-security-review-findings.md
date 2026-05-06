---
date: 2026-05-05 21:58:18 UTC
researcher: Sean Larkin
git_commit: 2defa916bba5e1a654ded2b64d5ab0bd32f746cd
branch: main
repository: aipm
topic: "Security review findings (issue #793): AzDO logging-command injection in --reporter ci-azure, lint path-containment gaps, NuGet publish hardening"
tags: [research, security, lint, reporter, ci-azure, marketplace, valid-tool-name, path-security, validated-path, import-resolver, discovery-walker, release, nuget, oidc, signing, issue-793]
status: complete
last_updated: 2026-05-05
last_updated_by: Sean Larkin
---

# Research

## Research Question

Document the current state of the three findings in [issue #793](https://github.com/TheLarkInn/aipm/issues/793):

1. **HIGH** — Azure DevOps logging-command injection via the `##[group]` header in `--reporter ci-azure`.
2. **MEDIUM** — `aipm lint` rules can read outside the project tree via PR-controlled inputs (`marketplace_source_resolve`, `marketplace_field_mismatch`, `valid_tool_name`).
3. **MEDIUM** — `.github/workflows/release-nuget.yml` retains a long-lived `NUGET_API_KEY` fallback, allows `workflow_dispatch`, and produces no independently signed artefact.

The issue body cites the audit commit `2b2a3822b926318ec46ec23a0cb44e2d63895b97`. This research re-verifies each finding against HEAD `2defa91`. Per the user, the document is strictly descriptive — it documents what IS, not what should be.

## Summary

All three findings reproduce at HEAD. The reporter, the two marketplace lint rules, and `release-nuget.yml` are byte-identical to the audit commit. `valid_tool_name.rs` has line-number drift only (commit `1375c4d` added test coverage; the parent-walking loop is unchanged in behaviour and now lives at `valid_tool_name.rs:206-216` instead of the audit's cited `:207-219`).

| # | Severity | Sink | Untrusted input | Helper that exists elsewhere | Helper applied here? |
|---|---|---|---|---|---|
| 1 | HIGH | `##[group]aipm lint: …` line at `lint/reporter.rs:351` | `Diagnostic.file_path` (PathBuf from filesystem walk) | `escape_azure_log_command` (`reporter.rs:397-403`) | **No** (the same path is escaped one line later for `sourcepath=…`) |
| 2a | MEDIUM | `fs.exists(&resolved)` at `marketplace_source_resolve.rs:103` | `marketplace.json` `plugins[].source` | `path_security::ValidatedPath`, `lint/rules/import_resolver::is_path_safe` | **No** containment check |
| 2b | MEDIUM | `fs.read_to_string(&pj_path)` at `marketplace_field_mismatch.rs:92` | `marketplace.json` `plugins[].source` | same as 2a | **No** containment check |
| 2c | MEDIUM | `fs.exists(&candidate)` at `valid_tool_name.rs:210` (loop ascends to filesystem root) | discovery-walker seed path's parent chain | discovery walker is symlink-safe (`follow_symlinks: false`) but no upper bound on the parent walk | **No** depth bound; loop terminates only on `Path::parent() == None` |
| 3 | MEDIUM | `dotnet nuget push` at `release-nuget.yml:182-185` (fallback) | `secrets.NUGET_API_KEY` | OIDC trusted publishing implemented at `:163-176` | OIDC and the long-lived secret coexist; no `environment:` reviewer gate; `dotnet nuget sign` / ESRP / sigstore / cosign / `actions/attest-build-provenance` all absent |

## Source Document

The triage issue: <https://github.com/TheLarkInn/aipm/issues/793> (filed 2026-05-05 by `@TheLarkInn`, no comments at time of research, no labels). The Body Cites Customer Security Review for adopting `aipm lint`.

## Note on Line-Number Drift Between Audit and HEAD

Diff between the audit commit `2b2a3822b…` and HEAD `2defa91` for cited files:

```
crates/libaipm/src/lint/reporter.rs                       no diff
crates/libaipm/src/lint/rules/marketplace_source_resolve.rs  no diff
crates/libaipm/src/lint/rules/marketplace_field_mismatch.rs  no diff
.github/workflows/release-nuget.yml                       no diff
crates/libaipm/src/lint/rules/valid_tool_name.rs          156 lines added (commit 1375c4d, test-only)
```

The single intervening behaviour-affecting commit, [`1375c4d`](https://github.com/TheLarkInn/aipm/commit/1375c4d) ("test(valid-tool-name): cover nearest_declared_engines error branches"), is test-only — no production code paths changed. The PR that landed between audit and HEAD ([#792](https://github.com/TheLarkInn/aipm/pull/792), merged 2026-05-05) touched none of these files.

## Detailed Findings

### Finding 1 — `##[group]` line in `--reporter ci-azure` interpolates `file_path` raw

#### Reporter surface

`crates/libaipm/src/lint/reporter.rs` defines the `Reporter` trait at [`reporter.rs:13-20`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L13-L20) with five impls:

| Mode | Type | `impl Reporter` |
|---|---|---|
| Plain text (library-only; not exposed at the CLI today) | `Text` | [`reporter.rs:25-49`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L25-L49) |
| Rich human (rustc/clippy-style, `annotate_snippets`) | `Human<'a>` | [`reporter.rs:106-133`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L106-L133) |
| JSON | `Json` | [`reporter.rs:241-303`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L241-L303) |
| GitHub Actions workflow commands | `CiGitHub` | [`reporter.rs:311-331`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L311-L331) |
| Azure DevOps `##vso[…]` log commands | `CiAzure` | [`reporter.rs:339-380`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L339-L380) |

The CLI advertises four reporter values — `human`, `json`, `ci-github`, `ci-azure` — at [`crates/aipm/src/main.rs:175-177`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/aipm/src/main.rs#L175-L177); the deprecated `--format text` alias is mapped to `human` at [`main.rs:765-768`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/aipm/src/main.rs#L765-L768).

#### Every line `CiAzure` writes

| Line shape | Where written | Interpolated fields | Escaped? |
|---|---|---|---|
| `##[group]aipm lint: <file_path>` | [`reporter.rs:351`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L351) | `d.file_path.display()` | **NO** — written raw |
| `##[endgroup]` (between groups) | [`reporter.rs:349`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L349) | static literal | n/a |
| `##[endgroup]` (after final group) | [`reporter.rs:371`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L371) | static literal | n/a |
| `##vso[task.logissue type=…;sourcepath=…;linenumber=…;columnnumber=…;code=…]<body>` | [`reporter.rs:364-367`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L364-L367) | severity, sourcepath, linenumber, columnnumber, code, body | severity is a closed enum; line/col are numeric; sourcepath, code, body each pass through `escape_azure_log_command` at [`:361-363`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L361-L363) |
| `##vso[task.complete result=SucceededWithIssues;]` | [`reporter.rs:375`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L375) | static literal | n/a |

The `##[group]` line at `:351`:

```rust
writeln!(writer, "##[group]aipm lint: {}", d.file_path.display())?;
```

is the only line in the reporter that interpolates a non-static, PR-author-controlled field without passing it through any escape helper.

#### Escape helpers in `reporter.rs`

| Helper | Defined | Sanitises | Call sites |
|---|---|---|---|
| `escape_github_prop` | [`reporter.rs:383-389`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L383-L389) | `%`→`%25`, `\r`→`%0D`, `\n`→`%0A`, `:`→`%3A`, `,`→`%2C` | `:321` (GitHub `file=` property) |
| `escape_github_message` | [`reporter.rs:392-394`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L392-L394) | `%`→`%25`, `\r`→`%0D`, `\n`→`%0A` | `:322` (rule_id), `:323` (message) |
| `escape_azure_log_command` | [`reporter.rs:397-403`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L397-L403) | `%`→`%AZP25`, `\r`→`%0D`, `\n`→`%0A`, `;`→`%3B`, `]`→`%5D` | `:361` (sourcepath), `:362` (code/rule_id), `:363` (body) |
| `escape_json_string` | [`reporter.rs:425-431`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L425-L431) | `\\`→`\\\\`, `"`→`\\"`, `\n`→`\\n`, `\r`→`\\r`, `\t`→`\\t` | `:259, :263, :268, :272, :294` (JSON reporter) |

`escape_azure_log_command` does not sanitise `:`, `,`, `[`, ANSI control sequences, or non-printable bytes other than `\r`/`\n`.

A helper, `format_azure_logissue_body` ([`reporter.rs:411-423`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L411-L423)), composes `"{rule_id}: {message}"` plus optional `" \u{2014} {help_text}"` and `" (see {help_url})"` from the `Diagnostic` and returns the unescaped composed string. The doc-comment at [`:406-410`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L406-L410) states the caller must escape the result; the only caller, [`reporter.rs:363`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L363), does so once via `escape_azure_log_command`.

#### `Diagnostic` field origins (PR-author controllability)

[`crates/libaipm/src/lint/diagnostic.rs:39-63`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/diagnostic.rs#L39-L63):

```rust
pub struct Diagnostic {
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    pub file_path: PathBuf,
    pub line: Option<usize>,
    pub col: Option<usize>,
    pub end_line: Option<usize>,
    pub end_col: Option<usize>,
    pub source_type: String,
    pub help_text: Option<String>,
    pub help_url: Option<String>,
}
```

- `file_path` — populated by individual rules from discovery-walker output (`PathBuf`, relative to the workspace root). Components originate from filesystem entries beneath the linted directory; on Linux/macOS, file names are bytes minus `/` and `\0`, so `\r`/`\n`/`;`/`]`/ANSI sequences are all valid.
- `rule_id` — rule constants (not user-controlled in normal flow).
- `message` — composed per rule; rule code may interpolate values read from PR-controlled files (see e.g. the test fixture at [`reporter.rs:457`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L457) which embeds an unknown hook event name into the message).
- `help_text`, `help_url` — written by `apply_rule_diagnostics` at [`lint/mod.rs:80-81`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/mod.rs#L80-L81) from rule-defined string constants.

#### Reporter dispatch (where `--reporter ci-azure` selects `CiAzure`)

[`crates/aipm/src/main.rs:802-820`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/aipm/src/main.rs#L802-L820):

```rust
match effective_reporter.as_str() {
    "json" =>     libaipm::lint::reporter::Json.report(&outcome, &mut stdout)?,
    "ci-github" => libaipm::lint::reporter::CiGitHub.report(&outcome, &mut stdout)?,
    "ci-azure" =>  libaipm::lint::reporter::CiAzure.report(&outcome, &mut stdout)?,
    _ => libaipm::lint::reporter::Human { … }.report(&outcome, &mut stdout)?,
}
```

#### Tests covering `CiAzure`

19 unit tests in `reporter.rs`, plus 4 helper-function tests, plus 1 CLI smoke test at [`main.rs:1790-1799`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/aipm/src/main.rs#L1790-L1799). One snapshot at `crates/libaipm/src/lint/snapshots/libaipm__lint__reporter__tests__ci_azure_sample_outcome_snapshot.snap`. Notable cases:

- `ci_azure_escape_newline_in_message` ([`:982-1013`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L982-L1013)) — verifies `\n` in message → `%0A` in body.
- `ci_azure_escape_semicolon_in_help_url` ([`:1015-1027`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L1015-L1027)) — verifies `;` in URL → `%3B`.
- `ci_azure_escape_bracket_in_help_text` ([`:1029-1041`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L1029-L1041)) — verifies `]` in help text → `%5D`.
- `ci_azure_group_per_file` ([`:1156-1203`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/reporter.rs#L1156-L1203)) — exercises the multi-file group/endgroup transitions, but the synthetic file paths (`a.md`, `b.md`) contain no special characters.

`grep -r ci-azure tests/features` returns no results — no Cucumber scenarios cover the reporter.

No existing test exercises a `Diagnostic.file_path` containing `\n`, `\r`, `]`, `;`, or ANSI escapes.

---

### Finding 2 — Lint rules construct paths from PR-controlled inputs without containment

#### Common surface

All three rules implement the `Rule` trait at [`crates/libaipm/src/lint/rule.rs:16-41`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rule.rs#L16-L41), whose check entry is:

```rust
fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, super::Error>;
```

The `Fs` trait at [`crates/libaipm/src/fs.rs:27-46`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/fs.rs#L27-L46) exposes only `exists`, `read_to_string`, `read_dir`. There is no `metadata`, `is_file`, `glob`, or `canonicalize` method on the trait. Rules are dispatched per-feature-kind via `quality_rules_for_kind` at [`crates/libaipm/src/lint/rules/mod.rs:124-175`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/mod.rs#L124-L175).

#### 2a. `marketplace/source-resolve`

File: `crates/libaipm/src/lint/rules/marketplace_source_resolve.rs`. Rule struct `SourceResolve` at [`:13-48`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/marketplace_source_resolve.rs#L13-L48). Default severity `Error`.

**Untrusted input.** Parsed at [`:55, 61`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/marketplace_source_resolve.rs#L55-L61) as raw `serde_json::Value`. Field path: `parsed["plugins"][i]["source"]` extracted at [`:72, 87, 93, 101`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/marketplace_source_resolve.rs#L72-L101) and bound as `source: &str`.

**Path construction** at [`:101-103`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/marketplace_source_resolve.rs#L101-L103):

```rust
let resolved = ai_dir.join(source.trim_start_matches("./"));
if !fs.exists(&resolved) {
```

`ai_dir` is `file_path.parent().and_then(|p| p.parent())` at [`:43`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/marketplace_source_resolve.rs#L43) — i.e. `.ai/` for a marketplace at `.ai/.claude-plugin/marketplace.json`. `trim_start_matches("./")` strips a literal prefix only.

**Containment check.** None. `..`, absolute paths, Windows drive prefixes, and encoded variants are all accepted. The `fs.exists(&resolved)` call (or `read_to_string` for a different rule) sees whatever path the join produced.

**Tests.** 13 unit tests at [`:120-271`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/marketplace_source_resolve.rs#L120-L271). None exercise a `..`-containing or absolute-path source value. Integration test `lint_marketplace_source_not_found_emits_diagnostic` at [`lint/mod.rs:1148-1166`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/mod.rs#L1148-L1166) uses `source: "./missing-plugin"` only.

**Registry.** Boxed for `FeatureKind::Marketplace` at [`rules/mod.rs:148`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/mod.rs#L148).

#### 2b. `marketplace/plugin-field-mismatch`

File: `crates/libaipm/src/lint/rules/marketplace_field_mismatch.rs`. Rule struct `FieldMismatch` at [`:13-46`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/marketplace_field_mismatch.rs#L13-L46). Default severity `Error`.

**Untrusted input.** Same `marketplace.json` `source` field at [`:83`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/marketplace_field_mismatch.rs#L83), plus `plugin.json` content at the constructed path.

**Path construction** at [`:89-90`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/marketplace_field_mismatch.rs#L89-L90):

```rust
let pj_path =
    ai_dir.join(source.trim_start_matches("./")).join(".claude-plugin").join("plugin.json");
```

**Path consumption.** `fs.read_to_string(&pj_path)` at [`:92`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/marketplace_field_mismatch.rs#L92).

**Containment check.** None. Same shape as 2a — the appended `.claude-plugin/plugin.json` literal segment doesn't bound traversal because the user-controlled segment is joined first.

**Tests.** 14 unit tests at [`:139-316`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/marketplace_field_mismatch.rs#L139-L316). All `source` values are `"./foo"`. None exercise traversal.

**Registry.** Boxed for `FeatureKind::Marketplace` at [`rules/mod.rs:149`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/mod.rs#L149).

#### 2c. `valid-tool-name` — parent walk for `aipm.toml`

File: `crates/libaipm/src/lint/rules/valid_tool_name.rs`. Rule struct `ValidToolName` at [`:26-82`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/valid_tool_name.rs#L26-L82). Default severity `Warning`.

**Untrusted input.** Two layers:
- The agent/skill/hook frontmatter `tools` field, parsed via `crate::frontmatter::parse(&content)` at [`:56`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/valid_tool_name.rs#L56) and split by `parse_tools` at [`:88-94`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/valid_tool_name.rs#L88-L94).
- The `[package].engines` / `[workspace].engines` fields read from whatever `aipm.toml` the parent walk locates.

**The parent-walking loop** — [`valid_tool_name.rs:206-216`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/valid_tool_name.rs#L206-L216) (audit cited `:207-219`; line shift is from test additions in commit `1375c4d`):

```rust
fn find_nearest_manifest(file_path: &Path, fs: &dyn Fs) -> Option<PathBuf> {
    let mut current = file_path.parent();
    while let Some(dir) = current {
        let candidate = dir.join("aipm.toml");
        if fs.exists(&candidate) {
            return Some(candidate);
        }
        current = dir.parent();
    }
    None
}
```

- Seed: `file_path.parent()` — the directory containing the file currently being linted, supplied by the discovery walker via [`lint/mod.rs:151-156`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/mod.rs#L151-L156).
- Loop bound: only `Path::parent() == None` (filesystem root). `Options::max_depth` ([`lint/mod.rs:218`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/mod.rs#L218)) bounds discovery — not this ascent.
- On match: `fs.read_to_string(&manifest_path)` at [`:192`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/valid_tool_name.rs#L192) → `crate::manifest::parse(&content)` at [`:195`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/valid_tool_name.rs#L195) → `effective_engines(...)` at [`:198`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/valid_tool_name.rs#L198). `EngineSet::empty()` is the default for any error path.

**Containment check.** None. The candidate paths produced by the ascent are never compared to the workspace root or the `lint::Options::dir` ([`lint/mod.rs:215`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/mod.rs#L215)).

**Tests.** 21 unit tests at [`:218-506`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/valid_tool_name.rs#L218-L506). Test seed path is always `.ai/p/agents/reviewer.md`; manifest is always at `.ai/p/aipm.toml`. None place an `aipm.toml` outside the linted root or test the ascent terminating at filesystem root. BDD: [`tests/features/lint/valid-tool-name.feature`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/tests/features/lint/valid-tool-name.feature) — 5 scenarios, none traversal-relevant.

**Registry.** Boxed for `FeatureKind::Skill`/`Agent`/`Hook` at [`rules/mod.rs:135, 139, 144`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/mod.rs#L135-L144).

#### Existing path-containment helpers (the "good models" the issue cites)

##### `path_security::ValidatedPath`

[`crates/libaipm/src/path_security.rs:36-37`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/path_security.rs#L36-L37):

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ValidatedPath(String);
```

Constructor `ValidatedPath::new` at [`:46-50`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/path_security.rs#L46-L50) calls `validate_plugin_path` at [`:79-109`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/path_security.rs#L79-L109) which rejects:

- empty string → `EmptyPath`
- contains `\0` → `PathTraversal`
- `Component::ParentDir` (any `..` segment) → `PathTraversal`
- `Component::Prefix(_) | Component::RootDir` → `AbsolutePath`
- lowercase `contains("..")` or `contains("%2e%2e")` → `PathTraversal` (catches encoded traversal and `foo..bar` literal-substring case; tested at [`:215-220`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/path_security.rs#L215-L220))

Production call sites are all in `acquirer.rs` and `spec.rs` (plugin-source acquisition):
- [`acquirer.rs:14, 78, 227`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/acquirer.rs#L78)
- [`spec.rs:14, 62, 162, 302, 350, 378`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/spec.rs#L302)

Not used by the lint module today.

##### `import_resolver::is_path_safe`

The issue references `crates/libaipm/src/import_resolver.rs:66-70`; the actual location is [`crates/libaipm/src/lint/rules/import_resolver.rs:66-71`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/import_resolver.rs#L66-L71) (under `lint/rules/`):

```rust
fn is_path_safe(path: &str) -> bool {
    use std::path::Component;
    Path::new(path)
        .components()
        .all(|c| !matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(..)))
}
```

Called twice in the same file at [`:120`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/import_resolver.rs#L120) (for `@…md` imports) and [`:129`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/import_resolver.rs#L129) (for relative markdown links). Used by `Oversized::check_file` ([`instructions_oversized.rs:62-137`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/instructions_oversized.rs#L62-L137)) when `resolve_imports == true`. Not used by any of the three flagged rules.

##### Discovery walker symlink handling

[`crates/libaipm/src/discovery/walker.rs:53-84`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/discovery/walker.rs#L53-L84):

```rust
let mut builder = ignore::WalkBuilder::new(project_root);
builder.hidden(false);
builder.git_ignore(true);
builder.git_global(true);
builder.git_exclude(true);
builder.follow_links(opts.follow_symlinks);
```

Backed by the `ignore` crate (same as ripgrep). `DiscoverOptions::default()` at [`discovery/mod.rs:42-51`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/discovery/mod.rs#L42-L51) leaves `follow_symlinks: false`. The lint pipeline hard-codes `follow_symlinks: false` at [`lint/mod.rs:151-156`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/mod.rs#L151-L156), and `migrate` does the same at [`migrate/unified.rs:438-446`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/migrate/unified.rs#L438-L446).

The walker's symlink stance applies to the *discovery* phase; the three flagged rules build paths *after* discovery from PR-author-controlled JSON/TOML content, so the walker's defence does not extend to them.

##### Other path-validation helpers in the workspace

| Helper | Defined | Behaviour | Used by |
|---|---|---|---|
| `is_safe_path_segment` | [`migrate/emitter.rs:15-22`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/migrate/emitter.rs#L15-L22) | rejects empty, `.`, `..`, `/`, `\`, absolute paths | `emitter.rs:39, 265, 274, 364, 376` |
| `is_relative_script` | [`migrate/hook_detector.rs:110-130`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/migrate/hook_detector.rs#L110-L130) | classifies relative-script-vs-PATH-name | hook detection in migrate |
| `has_windows_drive_prefix` | [`migrate/hook_detector.rs:134-140`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/migrate/hook_detector.rs#L134-L140) | catches `C:\…` cross-platform | `is_relative_script` |
| Inline check in `broken_paths` rule | [`broken_paths.rs:64-67`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/rules/broken_paths.rs#L64-L67) | rejects `''`, `starts_with('/')`, `contains("..")` before joining | only by `broken_paths` |

The `broken_paths` rule is notable: it is itself a lint rule that takes PR-controlled file content, but it does apply a containment check before joining. The two flagged marketplace rules do not.

##### Path-traversal BDD coverage that exists today

[`tests/features/security/path-traversal.feature`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/tests/features/security/path-traversal.feature) — 4 `@p0 @security` scenarios covering `Spec` parsing only:

1. "Directory traversal in git spec is rejected"
2. "URL-encoded traversal in spec is rejected"
3. "Absolute path in git spec is rejected"
4. "Null bytes in path are rejected"

No BDD scenarios cover lint-rule path containment.

---

### Finding 3 — `release-nuget.yml` retains long-lived secret fallback, no signing, no environment gate

#### Triggers

[`release-nuget.yml:13-21`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L13-L21):

```yaml
on:
  release:
    types: [published]
  workflow_dispatch:
    inputs:
      tag:
        description: 'Release tag to publish (e.g., aipm-v0.22.3). …'
        required: true
        type: string
```

`workflow_dispatch` is allowed without a branch restriction or environment reviewer.

#### Permissions

Top-level: none. Per-job, on the `publish` job at [`:36-38`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L36-L38):

```yaml
permissions:
  contents: read
  id-token: write   # required for NuGet Trusted Publishing (OIDC)
```

No `attestations:` permission.

#### Environment binding

None. The `publish` job ([`:28-39`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L28-L39)) has no `environment:` key. Step-level `env:` blocks at [`:49-51, 69-70`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L49-L51) are env vars, not GitHub deployment environments.

#### Concurrency / timeout

[`:23-25`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L23-L25):

```yaml
concurrency:
  group: release-nuget-${{ inputs.tag || github.event.release.tag_name }}
  cancel-in-progress: false
```

Job timeout `timeout-minutes: 20` at [`:39`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L39).

#### OIDC trusted-publishing wiring

[`:163-168`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L163-L168):

```yaml
- name: NuGet OIDC login (Trusted Publishing)
  id: nuget_login
  continue-on-error: true
  uses: NuGet/login@v1
  with:
    user: ${{ secrets.NUGET_USERNAME }}
```

The action is `NuGet/login@v1` with the only configured `with:` parameter being `user:`. No `audience:`, `subject:`, or token-specific configuration is set. `continue-on-error: true` means a failed OIDC exchange does not abort the job. The action's output `NUGET_API_KEY` is consumed at [`:174`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L174).

#### `NUGET_API_KEY` handling — OIDC vs long-lived secret

Both paths exist concurrently as adjacent steps:

OIDC push at [`:170-176`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L170-L176):

```yaml
- name: Push to nuget.org (OIDC)
  if: steps.nuget_login.outcome == 'success'
  run: |
    dotnet nuget push out/*.nupkg \
      --api-key "${{ steps.nuget_login.outputs.NUGET_API_KEY }}" \
      --source https://api.nuget.org/v3/index.json \
      --skip-duplicate
```

Fallback at [`:178-185`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L178-L185):

```yaml
- name: Push to nuget.org (API-key fallback)
  if: steps.nuget_login.outcome != 'success'
  run: |
    echo "::warning::OIDC Trusted Publishing failed; falling back to NUGET_API_KEY secret"
    dotnet nuget push out/*.nupkg \
      --api-key "${{ secrets.NUGET_API_KEY }}" \
      --source https://api.nuget.org/v3/index.json \
      --skip-duplicate
```

The fallback `if:` condition is `outcome != 'success'`, which fires on `failure`, `cancelled`, and `skipped` outcomes. Combined with `continue-on-error: true` on the OIDC step, any path that does not return `success` from `NuGet/login@v1` routes execution into the long-lived secret. NuGet.org's trusted-publisher binding is configured externally to the repo; the workflow itself does not assert on the binding scope.

#### Build, pack, publish chain (in source order)

| # | Step | Action / `run:` | Lines |
|---|---|---|---|
| 1 | `Checkout` (`persist-credentials: false`) | `actions/checkout@v4` | [42-45](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L42-L45) |
| 2 | `Resolve tag and version` | inline `run:` | [47-66](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L47-L66) |
| 3 | `Download release archives` | `gh release download` | [68-79](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L68-L79) |
| 4 | `Unpack into runtimes/<RID>/native layout` | inline `run:` | [81-111](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L81-L111) |
| 5 | `Set up .NET SDK` | `actions/setup-dotnet@v4`, `8.x` | [113-116](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L113-L116) |
| 6 | `Set up NuGet CLI` | `nuget/setup-nuget@v2` | [118-119](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L118-L119) |
| 7 | `Install mono` | `apt-get install mono-devel` | [121-130](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L121-L130) |
| 8 | `Pack` | `nuget pack ../packaging/aipm.nuspec …` | [132-146](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L132-L146) |
| 9 | `Inspect nupkg (sanity check)` | inline (≥ 4 native entries required) | [148-161](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L148-L161) |
| 10 | `NuGet OIDC login (Trusted Publishing)` | `NuGet/login@v1` | [163-168](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L163-L168) |
| 11 | `Push to nuget.org (OIDC)` | inline `dotnet nuget push` | [170-176](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L170-L176) |
| 12 | `Push to nuget.org (API-key fallback)` | inline `dotnet nuget push` | [178-185](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L178-L185) |

#### Signing / attestation — catalogue of absent items

A textual search of the file for these terms returns **no matches**:

- `dotnet nuget sign`
- `nuget sign`
- ESRP
- `sigstore`
- `cosign`
- `actions/attest`
- `actions/attest-build-provenance`
- `slsa`
- `crane`

The OIDC token mint by `NuGet/login@v1` authenticates the publisher; it does not produce a package-level signature or an external build-provenance attestation that downstream consumers can verify independently.

#### Tag / ref filtering

Job-level `if:` at [`:30-34`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L30-L34):

```yaml
if: >-
  ${{ github.event_name == 'workflow_dispatch'
      || (github.event_name == 'release'
          && startsWith(github.event.release.tag_name, 'aipm-v')
          && !github.event.release.prerelease) }}
```

For `workflow_dispatch`, the `aipm-v*` predicate is enforced inside the resolve-tag step at [`:59-62`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L59-L62). No branch filter is present at any level.

#### Adjacent release workflows

| File | Triggers | Purpose |
|---|---|---|
| [`release.yml`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release.yml) | `pull_request`, `push` of tag `**[0-9]+.[0-9]+.[0-9]+*` | autogenerated `cargo-dist` workflow; builds platform archives + installers, uploads to GitHub Release; `permissions: contents: write` |
| [`release-plz.yml`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-plz.yml) | `push` to `main` (excluding `**/*.md`, `LICENSE`) | `release-plz/action@v0.5`; opens release PR, merges, publishes to crates.io and tags |
| [`update-latest-release.yml`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/update-latest-release.yml) | `release: [published]` (gated on `aipm-v*`, non-prerelease) | copies installer scripts to `latest`-tagged release for stable README links |

#### Tests / dry-run safeguards

- No `--dry-run` flag anywhere in the file.
- `--source` is hard-coded to `https://api.nuget.org/v3/index.json` at [`:175, 184`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L175); no staging feed.
- `workflow_dispatch` accepts only `tag` (no `dry_run` input).
- The `Inspect nupkg` step at [`:148-161`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/.github/workflows/release-nuget.yml#L148-L161) validates ≥ 4 `runtimes/*/native/aipm[.exe]` entries before publishing — it is a pre-publish sanity check, not a no-op path.

---

## Code References

### Finding 1
- `crates/libaipm/src/lint/reporter.rs:351` — `##[group]aipm lint: {file_path.display()}` — sole un-escaped sink in the `CiAzure` reporter.
- `crates/libaipm/src/lint/reporter.rs:361-363` — `escape_azure_log_command` applied to `sourcepath`, `code`, `body`.
- `crates/libaipm/src/lint/reporter.rs:397-403` — `escape_azure_log_command` definition (`%`, `\r`, `\n`, `;`, `]` only).
- `crates/libaipm/src/lint/diagnostic.rs:39-63` — `Diagnostic` struct.
- `crates/aipm/src/main.rs:802-820` — reporter dispatch.
- `crates/aipm/src/main.rs:1790-1799` — `cmd_lint_ci_azure_reporter_succeeds_on_clean_dir` smoke test.

### Finding 2
- `crates/libaipm/src/lint/rules/marketplace_source_resolve.rs:101-103` — `ai_dir.join(source.trim_start_matches("./"))` then `fs.exists(&resolved)`.
- `crates/libaipm/src/lint/rules/marketplace_field_mismatch.rs:89-92` — chained `.join(source)…join(".claude-plugin").join("plugin.json")` then `fs.read_to_string(&pj_path)`.
- `crates/libaipm/src/lint/rules/valid_tool_name.rs:206-216` — parent walk searching for `aipm.toml` (no depth bound).
- `crates/libaipm/src/lint/rules/valid_tool_name.rs:188-200` — `nearest_declared_engines` reads and parses the located manifest.
- `crates/libaipm/src/path_security.rs:36-37, 79-109` — `ValidatedPath` and `validate_plugin_path`.
- `crates/libaipm/src/lint/rules/import_resolver.rs:66-71, 120, 129` — `is_path_safe` and its two call sites.
- `crates/libaipm/src/discovery/walker.rs:53-84` — walker construction (`follow_links(false)` by default).
- `crates/libaipm/src/lint/mod.rs:151-156` — lint pipeline hard-codes `follow_symlinks: false`.
- `crates/libaipm/src/lint/rules/broken_paths.rs:64-67` — example of an inline containment check inside a lint rule.
- `tests/features/security/path-traversal.feature` — existing path-traversal BDD scenarios (covers `Spec` parsing, not lint rules).

### Finding 3
- `.github/workflows/release-nuget.yml:13-21` — triggers (`release: published`, `workflow_dispatch`).
- `.github/workflows/release-nuget.yml:30-34` — job-level `if:` gate.
- `.github/workflows/release-nuget.yml:36-38` — `id-token: write` permission.
- `.github/workflows/release-nuget.yml:163-168` — `NuGet/login@v1` (`continue-on-error: true`).
- `.github/workflows/release-nuget.yml:170-176` — OIDC push (`if: outcome == 'success'`).
- `.github/workflows/release-nuget.yml:178-185` — `secrets.NUGET_API_KEY` push (`if: outcome != 'success'`).

## Architecture Documentation

- `Reporter` is a small trait with five impls; reporter selection is entirely in the CLI binary, not the library. Library-level wiring re-exports `reporter` as a public submodule at [`lint/mod.rs:11`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/lint/mod.rs#L11).
- Three escape helpers exist in `reporter.rs`, each scoped to one downstream sink (GitHub property, GitHub message, Azure log command); ANSI control sequences and `:`/`,`/`[` are not handled by any helper.
- The `Rule` trait operates exclusively through `Fs` ([`fs.rs:27-46`](https://github.com/TheLarkInn/aipm/blob/2defa916bba5e1a654ded2b64d5ab0bd32f746cd/crates/libaipm/src/fs.rs#L27-L46)), which exposes `exists`, `read_to_string`, `read_dir` — no `metadata`, `is_file`, `glob`, `canonicalize`. Adding a containment check at the rule layer would not require a trait extension.
- `ValidatedPath` is currently scoped to plugin-source acquisition (`acquirer.rs`/`spec.rs`); it is not imported by any module under `lint/`.
- `is_path_safe` is the only path-containment helper inside the lint module. `broken_paths` is the only lint rule that performs an inline containment check today.
- `release-nuget.yml` is a single-job workflow. OIDC and the long-lived API key are wired as parallel publish steps with mutually exclusive `if:` conditions on the OIDC login's `outcome`. No release workflow in the repo (`release.yml`, `release-plz.yml`, `release-nuget.yml`, `update-latest-release.yml`) emits an external signature or build-provenance attestation.

## Historical Context (from research/)

### Reporter / ci-azure
- [`research/docs/2026-04-20-azure-devops-lint-reporter-parity.md`](../docs/2026-04-20-azure-devops-lint-reporter-parity.md) — primary prior art on the ci-azure reporter; documents the existing escape table (`%→%AZP25`, `;→%3B`, `]→%5D`, `\r→%0D`, `\n→%0A`), the 5 `Diagnostic` fields not surfaced, and the broader `##vso` / `##[group]` / `##[error]` / `##[section]` / `task.uploadsummary` / `task.addattachment` protocol surface.
- [`research/docs/2026-03-31-110-aipm-lint-architecture-research.md`](../docs/2026-03-31-110-aipm-lint-architecture-research.md) — architecture research for `aipm lint` (issue #110); reporter trait context.
- [`research/docs/2026-04-02-aipm-lint-configuration-research.md`](../docs/2026-04-02-aipm-lint-configuration-research.md) — configuration / severity overrides.
- [`research/docs/2026-04-10-377-vscode-support-aipm-lint.md`](../docs/2026-04-10-377-vscode-support-aipm-lint.md) — discusses reporter formats from the VS Code consumer side.

### Marketplace lint rules
- [`research/docs/2026-04-07-lint-rules-287-288-289-290.md`](../docs/2026-04-07-lint-rules-287-288-289-290.md) — closest prior art on marketplace lint rules (issues #287–#290). Does not specifically address path containment.
- [`research/docs/2026-03-24-marketplace-description-mismatch-bug.md`](../docs/2026-03-24-marketplace-description-mismatch-bug.md), [`research/docs/2026-03-25-marketplace-name-customization-in-init.md`](../docs/2026-03-25-marketplace-name-customization-in-init.md), [`research/docs/2026-03-16-aipm-init-workspace-marketplace.md`](../docs/2026-03-16-aipm-init-workspace-marketplace.md) — adjacent marketplace context.

### `valid_tool_name` and `aipm.toml` engines
- [`research/tickets/2026-05-01-510-aipm-toml-engines.md`](2026-05-01-510-aipm-toml-engines.md) — explicit prior coverage of `agent/valid-tool-name` (#697), `aipm.toml` engines (#510), engine-aware init (#724).
- [`research/docs/2026-05-05-aipm-toml-engine-schema.md`](../docs/2026-05-05-aipm-toml-engine-schema.md), [`research/docs/2026-05-05-engine-catalog.md`](../docs/2026-05-05-engine-catalog.md) — engine schema / catalog. Neither addresses the parent-walk algorithm specifically.

### Path containment / `ValidatedPath`
- [`research/docs/2026-04-12-dry-rust-architecture-audit.md`](../docs/2026-04-12-dry-rust-architecture-audit.md) — calls out `ValidatedPath(String)` newtype as "correct and used"; lists `path_security.rs` and `security.rs` among inline-enum modules; notes a `Fs::symlink_dir` stub gap in `MockFs`.
- [`research/docs/2026-04-06-feature-status-audit.md`](../docs/2026-04-06-feature-status-audit.md) — marks "Path security (traversal)" as Implemented (`path_security.rs`, 20 tests).
- No prior research dedicated to `import_resolver` or to the discovery walker's symlink policy as a standalone topic.

### NuGet publishing
Cluster of four 2026-04-22 docs:
- [`research/docs/2026-04-22-nuget-publishing-pipeline.md`](../docs/2026-04-22-nuget-publishing-pipeline.md)
- [`research/docs/2026-04-22-github-actions-nuget-publish.md`](../docs/2026-04-22-github-actions-nuget-publish.md) — closest match to the live `release-nuget.yml` design.
- [`research/docs/2026-04-22-nuget-native-multi-rid-packaging.md`](../docs/2026-04-22-nuget-native-multi-rid-packaging.md)
- [`research/docs/2026-04-22-ado-pipeline-nuget-consume.md`](../docs/2026-04-22-ado-pipeline-nuget-consume.md)

Tangential: [`research/docs/2026-03-19-cargo-dist-installer-github-releases.md`](../docs/2026-03-19-cargo-dist-installer-github-releases.md), [`research/docs/2026-03-16-rust-cross-platform-release-distribution.md`](../docs/2026-03-16-rust-cross-platform-release-distribution.md). No prior research names ESRP, sigstore, cosign, OIDC trusted publishing, or build-provenance attestation as topics; they returned zero hits in `research/`.

### Security review / threat model
No prior research found. Mentions are confined to passing references in `research/docs/2026-03-09-aipm-cucumber-feature-spec.md` (a future "Security audit against advisory databases" feature) and `research/docs/2026-04-06-feature-status-audit.md` (notes `aipm audit` does not exist). No threat-model document exists at this revision.

## Related Research

- The reporter parity doc at [`research/docs/2026-04-20-azure-devops-lint-reporter-parity.md`](../docs/2026-04-20-azure-devops-lint-reporter-parity.md) is the most directly related prior art; any spec produced from this research will likely re-cite its escape-table coverage.
- The `path_security` audit notes in [`research/docs/2026-04-12-dry-rust-architecture-audit.md`](../docs/2026-04-12-dry-rust-architecture-audit.md) describe the helper that the issue cites as "the good model".
- The four 2026-04-22 NuGet docs together cover the design that produced `release-nuget.yml`; `2026-04-22-github-actions-nuget-publish.md` is the closest map to current code.

## Open Questions

The issue asks the team to file a fix decision. Items that remain unresolved at the research layer (i.e. the issue does not assert an answer):

- For Finding 1: whether file paths containing `\r`/`\n` are a real corpus concern in `aipm` consumers (the issue notes such filenames are legal on Linux/macOS file systems and inside zipped/tar'd checked-in payloads but does not estimate prevalence).
- For Finding 2: whether other lint rules (besides the three flagged here and `broken_paths`) read paths derived from PR-controlled inputs. A workspace-wide audit was not part of this research; the three flagged rules are the only ones the issue cites.
- For Finding 3: whether NuGet.org's trusted-publisher subject-claim binding for this repo is actually constrained to `repository_owner:TheLarkInn` + `ref:refs/tags/aipm-v*`. That binding lives outside the repo (in NuGet.org's publisher settings) and was not inspectable from the code.
- Whether `--reporter ci-github` (which uses different escapers and writes to a separate sink) has analogous gaps — the issue scope explicitly addresses only `ci-azure`.
