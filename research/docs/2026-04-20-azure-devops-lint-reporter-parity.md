---
date: 2026-04-20 17:41:53 UTC
researcher: Sean Larkin
git_commit: 3b340745e9c06ab0a2b37d2809ddc470cb2f8916
branch: main
repository: aipm
topic: "Azure DevOps lint reporter — parity with Human reporter and ADO pipeline surfaces"
tags: [research, codebase, lint, reporter, azure-devops, ci, diagnostics, developer-experience]
status: complete
last_updated: 2026-04-20
last_updated_by: Sean Larkin
---

# Research

## Research Question

What diagnostic context (snippets, help text/URLs, column spans, source-type, summary, rule-group metadata) does the Human reporter surface that the Azure DevOps (`CiAzure`) reporter currently omits, and what does the Azure DevOps `##vso` log-command protocol + task-summary artifacts natively support for carrying that context through to pipeline UI / developer troubleshooting?

## Summary

**Current state.** `aipm lint --reporter ci-azure` emits exactly one `##vso[task.logissue type=...;sourcepath=...;linenumber=...;columnnumber=...]<rule_id>: <message>` line per diagnostic. It surfaces 6 of the 11 `Diagnostic` fields (`rule_id`, `severity`, `message`, `file_path`, `line`, `col`) and drops 5: `end_line`, `end_col`, `source_type`, `help_text`, `help_url`. It writes no header, no per-file grouping, and no summary tail. The `Human` reporter surfaces 9 of the 11 fields (adds `help_text`, `help_url`, `end_col`, and a 3-line source snippet from `Fs`; still omits `end_line` and `source_type`) and appends a summary tail. Today, 18 of the 18 `Rule` implementations override `Rule::help_text()` / `Rule::help_url()`, so every diagnostic reaching any reporter has non-`None` help context — but `CiAzure` discards it.

**ADO protocol surfaces.** The Azure DevOps agent parses a richer set of commands than `CiAzure` currently uses:

- `##vso[task.logissue]` accepts a **fifth `code=` property** not set today (Microsoft-Learn doc verbatim).
- `##[group] ... ##[endgroup]`, `##[error]`, `##[warning]`, `##[section]` formatting commands create collapsible, severity-styled sections in the log pane.
- `##vso[task.uploadsummary]` attaches a markdown file to the run's Extensions tab (shorthand for `##vso[task.addattachment type=Distributedtask.Core.Summary;name=...]`).
- `##vso[task.uploadfile]` attaches raw files alongside the step log (good for `diagnostics.jsonl`).
- `##vso[task.addattachment]` with custom `type` stores timeline blobs reachable via the Attachments REST API (agent-consumable stream).
- SARIF 2.1.0 files dropped into a Build Artifact named `CodeAnalysisLogs` are rendered with clickable source line annotations by the `sariftools.scans` extension (and ingested by Defender for Cloud / GitHub Advanced Security for Azure DevOps).
- `task.logissue` alone does **not** create PR-line comments. PR annotations require either GHAzDO (via `AdvancedSecurity-Publish@1`) or a custom REST call to `POST /pullRequests/{id}/threads`.
- ANSI SGR codes are rendered in the modern log viewer (but `##[error]` / `##[warning]` are the portable, viewer-agnostic way to color).

**Gap.** Every piece of richer context the `Human` reporter prints to a terminal — help message, help URL, column spans, source snippet, per-file grouping, summary — has a first-class ADO surface that the current `CiAzure` reporter does not use. Help text could go into the `logissue` message body; the rule id could double as `code=`; the 3-line snippet + summary could live in an `uploadsummary` markdown blob; a machine-readable JSONL/SARIF dump could ride along via `uploadfile` / `addattachment` / `CodeAnalysisLogs`.

## Detailed Findings

### 1. `Diagnostic` and `Outcome` — the data the reporters consume

The `Diagnostic` struct at [`crates/libaipm/src/lint/diagnostic.rs:40-63`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/diagnostic.rs#L40-L63) carries 11 fields:

| Field | Type | Line |
|---|---|---|
| `rule_id` | `String` | [42](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/diagnostic.rs#L42) |
| `severity` | `Severity` | [44](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/diagnostic.rs#L44) |
| `message` | `String` | [46](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/diagnostic.rs#L46) |
| `file_path` | `PathBuf` | [48](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/diagnostic.rs#L48) |
| `line` | `Option<usize>` | [50](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/diagnostic.rs#L50) |
| `col` | `Option<usize>` | [52](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/diagnostic.rs#L52) |
| `end_line` | `Option<usize>` | [54](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/diagnostic.rs#L54) |
| `end_col` | `Option<usize>` | [56](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/diagnostic.rs#L56) |
| `source_type` | `String` | [58](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/diagnostic.rs#L58) |
| `help_text` | `Option<String>` | [60](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/diagnostic.rs#L60) |
| `help_url` | `Option<String>` | [62](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/diagnostic.rs#L62) |

`Outcome` ([`crates/libaipm/src/lint/mod.rs:183-193`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/mod.rs#L183-L193)) holds `diagnostics`, `error_count`, `warning_count`, and `sources_scanned: Vec<String>`.

#### How `help_text` / `help_url` are populated

Rules do **not** set these fields when constructing a `Diagnostic`. Every rule emits `help_text: None` / `help_url: None`, and [`apply_rule_diagnostics`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/mod.rs#L58-L67) at `lint/mod.rs:58-67` stamps them from `rule.help_text()` / `rule.help_url()` (Rule trait at [`crates/libaipm/src/lint/rule.rs:27-34`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/rule.rs#L27-L34)):

```rust
for mut d in rule_diagnostics {
    d.severity   = effective_severity;
    d.help_text  = rule.help_text().map(String::from);
    d.help_url   = rule.help_url().map(String::from);
    diagnostics.push(d);
}
```

Rules overriding both methods (every rule file in `crates/libaipm/src/lint/rules/`):

| Rule file | `help_url` | `help_text` |
|---|---|---|
| `agent_missing_tools.rs` | L26 | L30 |
| `broken_paths.rs` | L35 | L39 |
| `hook_legacy_event.rs` | L30 | L34 |
| `hook_unknown_event.rs` | L31 | L35 |
| `instructions_oversized.rs` | L54 | L58 |
| `marketplace_field_mismatch.rs` | L28 | L32 |
| `marketplace_source_resolve.rs` | L28 | L34 |
| `misplaced_features.rs` | L36 | L40 |
| `plugin_missing_manifest.rs` | L29 | L33 |
| `plugin_missing_registration.rs` | L29 | L33 |
| `plugin_required_fields.rs` | L28 | L32 |
| `skill_desc_too_long.rs` | L31 | L37 |
| `skill_invalid_shell.rs` | L31 | L35 |
| `skill_missing_desc.rs` | L26 | L30 |
| `skill_missing_name.rs` | L26 | L30 |
| `skill_name_invalid.rs` | L42 | L46 |
| `skill_name_too_long.rs` | L31 | L35 |
| `skill_oversized.rs` | L31 | L35 |

Example (`misplaced_features.rs` returns dynamic help text based on `self.ai_exists`):

```rust
fn help_url(&self) -> Option<&'static str> {
    Some("https://github.com/TheLarkInn/aipm/blob/main/docs/rules/source/misplaced-features.md")
}

fn help_text(&self) -> Option<&'static str> {
    if self.ai_exists {
        Some("run \"aipm migrate\" to move into the .ai/ marketplace")
    } else {
        Some("run \"aipm init\" to create a marketplace, then \"aipm migrate\"")
    }
}
```

Net effect: **every diagnostic that reaches a reporter already has `help_text: Some(...)` and `help_url: Some(...)` set.** These are stamped by the library layer, not by the reporter.

#### How `source_type` is populated

Derived from `scan::source_type_from_path(file_path)` at [`crates/libaipm/src/lint/rules/scan.rs:33-44`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/rules/scan.rs#L33-L44), which walks the path and returns one of `.ai`, `.claude`, `.github`, or `other`.

### 2. CLI reporter dispatch

Reporter selection lives in [`crates/aipm/src/main.rs:708-788`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/aipm/src/main.rs#L708-L788) (`cmd_lint`). The clap arg definition at [`main.rs:168-170`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/aipm/src/main.rs#L168-L170) whitelists `human`, `json`, `ci-github`, `ci-azure`. At [`main.rs:761-779`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/aipm/src/main.rs#L761-L779) a match constructs the corresponding zero-sized reporter:

```rust
match effective_reporter {
    "json"      => libaipm::lint::reporter::Json.report(&outcome, &mut stdout)?,
    "ci-github" => libaipm::lint::reporter::CiGitHub.report(&outcome, &mut stdout)?,
    "ci-azure"  => libaipm::lint::reporter::CiAzure.report(&outcome, &mut stdout)?,
    _           => {
        let human = libaipm::lint::reporter::Human {
            fs: &libaipm::fs::Real,
            color: color_choice,
            base_dir: &dir,
        };
        human.report(&outcome, &mut stdout)?;
    },
}
```

There is no per-reporter configuration struct or options — each reporter is instantiated with default behavior, and `CiAzure` / `CiGitHub` take no fields at all.

### 3. Human reporter surface (what CiAzure is being compared against)

File: [`crates/libaipm/src/lint/reporter.rs:97-236`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/reporter.rs#L97-L236). Built on `annotate_snippets` (rustc-style rendering).

For each diagnostic the `Human` reporter writes:

1. **Headline** — `error[rule_id]: message` or `warning[rule_id]: message`, via `level.title(&d.message).id(&d.rule_id)` at [L205](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/reporter.rs#L205).
2. **Origin arrow** — `--> <file_path>` always attached via `.origin(&origin)` at L212 / L217, even when the source file cannot be read.
3. **Source snippet** — up to 3 context lines ([L158-L160](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/reporter.rs#L158-L160)): previous line + target line + next line. Source is loaded via `self.fs.read_to_string(self.base_dir.join(&d.file_path)).ok()` ([L148-L149](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/reporter.rs#L148-L149)). Gutter line numbers match the real file via `.line_start(start_idx + 1)`.
4. **Column caret** — `level.span(span_start..span_end)` underlines the exact columns. Three branches at [L179-L190](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/reporter.rs#L179-L190): both `col`+`end_col` (precise range), `col` only (single char), neither (whole line).
5. **Help footer** — `Level::Help.title(help_text)` appended via `.footer(...)` at [L223](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/reporter.rs#L223).
6. **Help URL footer** — wrapped in `format!("for further information visit {help_url}")` at [L227-L228](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/reporter.rs#L227-L228).
7. **Summary tail** — `no issues found`, `warning: N warning(s) emitted`, `error: N error(s) emitted` at [L115-L129](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/reporter.rs#L115-L129).

ANSI color resolves via `ColorChoice::should_color()` ([L72-L90](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/reporter.rs#L72-L90)) honoring `NO_COLOR`, `CLICOLOR=0`, and `anstyle_query::term_supports_ansi_color()`.

#### Diagnostic → Human surface mapping

| Field | Surface in Human |
|---|---|
| `rule_id` | Bracketed id in headline (`error[id]:`). |
| `severity` | Headline level; also in summary tail. |
| `message` | Headline body. |
| `file_path` | `-->` origin label (every diagnostic, snippet or not). |
| `line` | Gutter line numbers; drives snippet window. |
| `col` | Caret start position. |
| `end_line` | **Not surfaced.** |
| `end_col` | Caret end position. |
| `source_type` | **Not surfaced.** |
| `help_text` | `Level::Help` footer. |
| `help_url` | Second `Level::Help` footer as `"for further information visit <url>"`. |
| snippet | Up to 3 lines loaded via `Fs`. |

### 4. CiAzure reporter surface — what's currently emitted

File: [`crates/libaipm/src/lint/reporter.rs:337-359`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/reporter.rs#L337-L359). Zero-sized struct; entire body is a single loop writing one line per diagnostic:

```rust
writeln!(
    writer,
    "##vso[task.logissue type={severity};sourcepath={sourcepath};linenumber={line};columnnumber={col}]{rule_id}: {message}",
)?;
```

Behavior:

- **Severity mapping** ([L342-L345](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/reporter.rs#L342-L345)): `Error => "error"`, `Warning => "warning"`. Two-level only.
- **None-line/col default** ([L346-L347](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/reporter.rs#L346-L347)): both `unwrap_or(1)`.
- **Escape** ([L376-L382](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/reporter.rs#L376-L382)): `%→%AZP25`, `\r→%0D`, `\n→%0A`, `;→%3B`, `]→%5D`. Applied to `sourcepath`, `rule_id`, `message`. Severity/line/col are not escaped (literals).
- **No header, no summary, no grouping.** Empty `diagnostics` produces zero bytes (verified by test `ci_azure_empty_diagnostics` at [L704-L716](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/reporter.rs#L704-L716)).
- **Outcome metadata ignored** — `outcome.error_count`, `outcome.warning_count`, and `outcome.sources_scanned` are never read.

#### Diagnostic → CiAzure surface mapping

| Field | Surface in CiAzure |
|---|---|
| `rule_id` | Emitted after `]`, escaped. |
| `severity` | Mapped to `type=`. |
| `message` | Emitted after `: `, escaped. |
| `file_path` | `sourcepath=`, escaped. |
| `line` | `linenumber=` (defaults to `1` when `None`). |
| `col` | `columnnumber=` (defaults to `1` when `None`). |
| `end_line` | **Dropped.** |
| `end_col` | **Dropped.** |
| `source_type` | **Dropped.** |
| `help_text` | **Dropped.** |
| `help_url` | **Dropped.** |

Verbatim test assertions at [reporter.rs:700-701](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/reporter.rs#L700-L701):

```
##vso[task.logissue type=warning;sourcepath=.ai/my-plugin/skills/default/SKILL.md;linenumber=1;columnnumber=1]skill/missing-description
##vso[task.logissue type=error;sourcepath=.ai/my-plugin/hooks/hooks.json;linenumber=5;columnnumber=1]hook/unknown-event
```

### 5. Sibling: CiGitHub reporter for comparison

[`reporter.rs:309-331`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/reporter.rs#L309-L331). Identical field-coverage profile to `CiAzure` — surfaces the same 6 fields and drops the same 5. Format:

```
::{severity} file={file},line={line},col={col}::{rule_id}: {message}
```

Escapes differ: `escape_github_prop` (for `file`) escapes `%→%25`, `\r→%0D`, `\n→%0A`, `:→%3A`, `,→%2C`; `escape_github_message` (for `rule_id` + `message`) escapes `%→%25`, `\r→%0D`, `\n→%0A`. The `%` sentinel differs from Azure (`%AZP25` vs `%25`).

### 6. Azure DevOps pipeline protocol — what ADO natively supports

Sourced from [Logging commands — Azure Pipelines (Microsoft Learn)](https://learn.microsoft.com/en-us/azure/devops/pipelines/scripts/logging-commands?view=azure-devops), the agent source [`CommandStringConvertor.cs`](https://github.com/microsoft/azure-pipelines-agent/blob/master/src/Agent.Sdk/CommandStringConvertor.cs), and community documentation.

#### 6a. `##vso[task.logissue]` — complete parameter list

> Properties
> - `type` = `error` or `warning` (Required)
> - `sourcepath` = source file location
> - `linenumber` = line number
> - `columnnumber` = column number
> - `code` = error or warning code

The `code=` property is **accepted but not currently set by `CiAzure`**. There is no `endline`, `endcolumn`, `severity`-beyond-error/warning, `helpuri`, `fingerprint`, or `rulename` field in the spec — these are not extensible via `logissue` alone.

Canonical example from docs:

```
##vso[task.logissue type=warning;sourcepath=consoleapp/main.cs;linenumber=1;columnnumber=1;code=100;]Found something that could be a problem.
```

Value-escaping rules from the same doc match what the current `escape_azure_log_command` implements (`;→%3B`, `\n→%0A`, `\r→%0D`, `]→%5D`, `%→%AZP25`).

#### 6b. Issues tab rendering

- Each `task.logissue type=error` becomes a red row; `type=warning` becomes yellow.
- Title format: `sourcepath(linenumber,columnnumber): <message>` (MSBuild-style).
- The `code` field appears MSBuild-style (e.g. `warning CS0168:`).
- Clicking an issue jumps to the matching line in the **step log**, not the source file. ADO does not make `sourcepath` clickable to the repo.
- Severity is binary — no info/hint tier.

#### 6c. Formatting commands (not currently used)

From the same Microsoft Learn doc:

```
##[group]Beginning of a group
##[warning]Warning message
##[error]Error message
##[section]Start of a section
##[debug]Debug text
##[command]Command-line being run
##[endgroup]
```

`##[group] ... ##[endgroup]` renders collapsible sections. They are not nestable (open feature request [Developer Community 10463480](https://developercommunity.visualstudio.com/t/Azure-Pipelines-Nested-Log-Formatting-Gr/10463480)). `##[error]` / `##[warning]` style log lines with severity colors without emitting raw ANSI.

#### 6d. Summary / attachment surfaces

- **`##vso[task.uploadsummary]<absolute path>`** — attaches a markdown file to the run's Extensions tab. Shorthand for `##vso[task.addattachment type=Distributedtask.Core.Summary;name=<name>;]<path>`. Multiple summaries per run are supported via `addattachment` directly. Must be UTF-8 or ASCII. Supports CommonMark: headings, links, tables, fenced code blocks, images, blockquotes.
- **`##vso[task.uploadfile]<absolute path>`** — attaches a file to the step's "Download logs" bundle. Opaque to the agent; good for `diagnostics.jsonl` or `results.sarif`.
- **`##vso[task.addattachment type=...;name=...]<path>`** — stores arbitrary timeline blobs. Custom types are reachable via the [Attachments REST API](https://learn.microsoft.com/en-us/rest/api/azure/devops/build/attachments/list) but only rendered in the UI if a marketplace extension is registered against that type. Well-known recognized types: `Distributedtask.Core.Summary` (markdown summary), `Sarif` (SARIF blob), `Distributedtask.Core.TaskLog`.
- **`##vso[task.complete result=SucceededWithIssues;]`** — marks step yellow-with-issues (distinct from red error).
- **`##vso[task.setvariable variable=... isOutput=true]`** — surface machine-readable counts (e.g. `aipm_error_count`) to downstream jobs.
- **`##vso[task.logdetail]`** — per Microsoft Learn, "they won't typically be shown in the UI" — primarily internal.
- **`##vso[build.uploadlog]<path>`** — uploads a log file to `logs\tool` container.

Important: *Logging commands are parsed only from step stdout on the agent — they are not parsed from uploaded files, test attachments, or SARIF.*

#### 6e. SARIF ingestion paths

1. **SARIF SAST Scans Tab extension** ([marketplace](https://marketplace.visualstudio.com/items?itemName=sariftools.scans), repo [microsoft/sarif-azuredevops-extension](https://github.com/microsoft/sarif-azuredevops-extension)) — publish `*.sarif` to a **Build Artifact named `CodeAnalysisLogs`** via `PublishBuildArtifacts@1`. Renders a Scans tab with clickable source line annotations (powered by `sarif-web-component`). Free extension.
2. **Microsoft Security DevOps (`MicrosoftSecurityDevOps@1`)** — auto-publishes SARIF to `CodeAnalysisLogs`. Ingested by Defender for Cloud ([configure docs](https://learn.microsoft.com/en-us/azure/defender-for-cloud/configure-azure-devops-extension)).
3. **GitHub Advanced Security for Azure DevOps (`AdvancedSecurity-Publish@1`)** — paid SKU. Surfaces findings in the Advanced Security blade + **PR diff annotations**. Requires SARIF 2.1.0. Limits: 20 runs/file, 5000 results/run, 100 locations/result, 10 tags/rule.

Per the [third-party ingestion docs](https://learn.microsoft.com/en-us/azure/devops/repos/security/github-advanced-security-code-scanning-third-party?view=azure-devops), publishing to `CodeAnalysisLogs` is the de-facto convention.

#### 6f. PR annotations — NOT emitted by `task.logissue`

The Issues tab is build-scoped. PR-diff annotations require one of:

1. GHAzDO via `AdvancedSecurity-Publish@1` (paid, SARIF-based).
2. Defender for Cloud PR annotations (IaC scans only as of 2026).
3. Self-rolled via [Pull Request Threads REST API](https://learn.microsoft.com/en-us/rest/api/azure/devops/git/pull-request-threads) — `POST /pullRequests/{id}/threads` with `threadContext.filePath`, `threadContext.rightFileStart/End.line/offset`. Called from the pipeline using `$(System.AccessToken)`. Community patterns: [Thomas Thornton](https://thomasthornton.cloud/2024/01/18/adding-pull-request-comments-to-azure-devops-repo-from-azure-devops-pipelines/), [Peter Moorey](https://petermoorey.github.io/adding-azure-pipeline-comments/), [Cloudlumberjack](https://cloudlumberjack.com/posts/ado-pr-psscriptanalyzer/).
4. Marketplace wrappers: [PR Comment Task](https://marketplace.visualstudio.com/items?itemName=TommiLaukkanen.pr-comment-extension), [CSE-DevOps Create Pull Request Comment](https://marketplace.visualstudio.com/items?itemName=CSE-DevOps.create-pr-comment-task).

Underlying client lib: `Microsoft.TeamFoundation.SourceControl.WebApi` (`GitHttpClient.CreateThreadAsync`).

#### 6g. ANSI color in log pane

- The modern Pipelines log viewer **does** render SGR foreground colors, bold, and reset. The old Classic view strips them.
- No `##vso` command toggles ANSI processing — always on in the new viewer.
- Each newline resets ANSI state; multi-line colored output needs escapes re-emitted at each line start.
- The `##[error]` / `##[warning]` / `##[section]` markers are the portable alternative that degrades gracefully.
- References: [Developer Community ANSI idea](https://developercommunity.visualstudio.com/idea/365961/support-ansi-escape-codes-for-color-in-build-outpu.html), [azure-pipelines-agent #1569](https://github.com/microsoft/azure-pipelines-agent/issues/1569), [Medium — Azure DevOps ANSI Colour Coding](https://medium.com/@paul-mackinnon/azure-devops-ansi-colour-coding-df8ef7406422).

### 7. Concrete precedent: other tools emitting rich ADO output

- **`eslint-formatter-azure-devops`** ([npm](https://www.npmjs.com/package/eslint-formatter-azure-devops), [repo](https://github.com/EngageSoftware/eslint-formatter-azure-devops)) — one `task.logissue` per finding, uses `code=<rule-name>` (or `code=null`), and optionally emits `##vso[task.complete result=SucceededWithIssues;]` at end.
- **MSBuild / dotnet** — emits `<source>(<line>,<col>): <severity> <code>: <message>` format; the ADO agent's MSBuild/VSBuild tasks pattern-match and auto-convert to `task.logissue`. `DotNetCoreCLI@2` does not fully replicate the conversion ([#9957](https://github.com/microsoft/azure-pipelines-tasks/issues/9957)).
- **PSRule** ([analysis output docs](https://microsoft.github.io/PSRule/v2/analysis-output/)) — emits both `task.logissue` and SARIF in parallel, combining Issues tab + SARIF Scans tab.
- **MegaLinter** ([example](https://aammir-mirza.medium.com/megalinter-with-azure-devops-e88526db7783)) — SARIF + REST-API PR comments + `##[group]` per linter.

### 8. Gap table — Human vs CiAzure vs "what ADO supports"

| Context | `Human` | `CiAzure` today | ADO native surface available |
|---|---|---|---|
| Rule id | headline `[rule_id]` | after `]` in message | message body + `code=` property |
| Severity | colored headline | `type=error/warning` | same; plus `##[error]/##[warning]` for log styling |
| Message | headline body | after `: ` | message body |
| File path | `-->` origin | `sourcepath=` | same; also in markdown summary |
| Line | gutter numbers | `linenumber=` | same |
| Column | caret position | `columnnumber=` | same |
| End line | **not surfaced** | **dropped** | **unsupported by logissue**; SARIF `region.endLine` |
| End column | caret end | **dropped** | **unsupported by logissue**; SARIF `region.endColumn`; markdown summary |
| Source type (`.ai` / `.claude` / `.github` / `other`) | **not surfaced** | **dropped** | markdown summary section headers, `##[group]` labels, SARIF `taxa` |
| Help text | `Level::Help` footer | **dropped** | message body suffix; markdown summary body; SARIF `message.text` |
| Help URL | 2nd footer | **dropped** | message body suffix (log viewer auto-linkifies URLs); markdown summary; SARIF `rule.helpUri` |
| Source snippet (±1 line) | rustc-style snippet via `Fs` | **dropped** | markdown summary fenced code block; SARIF `region.snippet` |
| Summary tail (`N error(s) emitted`) | printed | **dropped** | markdown summary (counts); step annotation via `uploadsummary` |
| File grouping | file per diagnostic | **dropped** | `##[group]file` + `##[endgroup]` |
| Color | ANSI (opt-in via `ColorChoice`) | n/a | `##[error]/##[warning]`; raw ANSI in modern viewer |
| Machine-readable stream for agents | n/a | **none** | `task.uploadfile diagnostics.jsonl`; `task.addattachment type=aipm.diagnostics.v1`; SARIF in `CodeAnalysisLogs` artifact |
| PR-diff annotations | n/a | **none** | GHAzDO `AdvancedSecurity-Publish@1`; REST `pullRequests/{id}/threads` |
| Status (warnings → yellow step) | exit code | exit code from CLI | `##vso[task.complete result=SucceededWithIssues;]` |

### 9. Rule-metadata surfaces already available

Because `apply_rule_diagnostics` stamps `help_text` and `help_url` onto every diagnostic, these fields are already carried through the pipeline:

- Every `Diagnostic` reaching `CiAzure` has `help_text: Some("<fix text>")` and `help_url: Some("<url>")` (18 rules × both overrides).
- The `Rule` trait ([`crates/libaipm/src/lint/rule.rs:27-34`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/rule.rs#L27-L34)) exposes `id()`, `name()`, `default_severity()`, `help_url()`, `help_text()` — more than the reporter layer currently consumes.

No reporter field is plumbed differently per rule group beyond what `source_type` and `rule_id` already encode.

### 10. BDD / integration test coverage

Grep of `tests/features/**/*.feature` for `--reporter`, `--format`, `ci-github`, `ci-azure`, `vso[task`, `::error`, `::warning` returned **zero matches**. `tests/features/guardrails/quality.feature` exercises `aipm lint` command-level behavior only (phrased as prose assertions like `Then a warning is reported: "SKILL.md missing required field: description"`).

CiAzure reporter coverage lives entirely in Rust unit tests at [`reporter.rs:694-742`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/crates/libaipm/src/lint/reporter.rs#L694-L742):

- `ci_azure_error_format` — format smoke test against `sample_outcome()`.
- `ci_azure_empty_diagnostics` — empty `Outcome` yields zero bytes.
- `ci_azure_defaults_line_col` — `None` line/col defaults to `1`.
- `ci_azure_with_col` — col=5 round-trips into `columnnumber=5`.

## Code References

- `crates/libaipm/src/lint/reporter.rs:13-20` — `Reporter` trait.
- `crates/libaipm/src/lint/reporter.rs:22-57` — `Text` reporter.
- `crates/libaipm/src/lint/reporter.rs:60-91` — `ColorChoice` enum + `should_color()`.
- `crates/libaipm/src/lint/reporter.rs:97-236` — `Human` reporter.
- `crates/libaipm/src/lint/reporter.rs:239-303` — `Json` reporter.
- `crates/libaipm/src/lint/reporter.rs:309-331` — `CiGitHub` reporter.
- `crates/libaipm/src/lint/reporter.rs:337-359` — `CiAzure` reporter.
- `crates/libaipm/src/lint/reporter.rs:376-382` — `escape_azure_log_command`.
- `crates/libaipm/src/lint/reporter.rs:694-742` — `CiAzure` unit tests.
- `crates/libaipm/src/lint/diagnostic.rs:7-22` — `Severity` enum + `Display`.
- `crates/libaipm/src/lint/diagnostic.rs:29-35` — `Severity::from_str_config`.
- `crates/libaipm/src/lint/diagnostic.rs:39-63` — `Diagnostic` struct.
- `crates/libaipm/src/lint/mod.rs:58-67` — `apply_rule_diagnostics` stamps `help_text`/`help_url`.
- `crates/libaipm/src/lint/mod.rs:120` — `pub fn lint`.
- `crates/libaipm/src/lint/mod.rs:170-193` — `Options` + `Outcome`.
- `crates/libaipm/src/lint/rule.rs:27-34` — `Rule::help_text()` / `Rule::help_url()` defaults.
- `crates/libaipm/src/lint/rules/scan.rs:33-44` — `source_type_from_path`.
- `crates/libaipm/src/lint/rules/misplaced_features.rs:23-67` — example rule with dynamic `help_text`.
- `crates/aipm/src/main.rs:159-183` — `Lint` clap args (`--reporter`, `--color`, `--format` legacy, `--max-depth`).
- `crates/aipm/src/main.rs:708-788` — `cmd_lint` dispatch.
- `crates/aipm/src/main.rs:761-779` — reporter instantiation match.

## Architecture Documentation

The lint output pipeline is a straight pipeline:

```
aipm lint <dir>  ──►  libaipm::lint::lint(opts, fs)
                         │
                         ▼
                      collect Rules  ──►  Rule::check_file → Vec<Diagnostic>
                         │
                         ▼
                   apply_rule_diagnostics  (stamps help_text, help_url, severity)
                         │
                         ▼
                      Outcome { diagnostics, error_count, warning_count, sources_scanned }
                         │
                         ▼
                cmd_lint match effective_reporter
                         │
        ┌────────────────┼────────────────┬───────────────┐
        ▼                ▼                ▼               ▼
     Human           Json             CiGitHub        CiAzure
  (rich terminal) (structured     (::error /     (##vso[task
                   JSON w/          ::warning)    .logissue])
                   severity_code,
                   help_url,
                   help_text)
```

Key architectural notes:

1. **Reporters are zero-config (except Human).** Only `Human` holds fields (`fs`, `color`, `base_dir`). `CiAzure`/`CiGitHub`/`Json` are zero-sized.
2. **Single write sink.** `Reporter::report(&self, &Outcome, &mut dyn Write) -> std::io::Result<()>` — reporters can only produce one byte stream.
3. **No Fs access for CI reporters.** `CiAzure` has no way to load source snippets even if it wanted to — that requires the `Fs` trait.
4. **Summaries are per-reporter.** `Text` and `Human` emit summary tails; `Json` emits a `summary` object; `CiGitHub` and `CiAzure` emit nothing after the last diagnostic.
5. **No multi-artifact output.** Reporters can only write to the single `&mut dyn Write` passed in. Emitting both a log stream (`##vso[task.logissue]`) and a separate markdown summary file would require either (a) the reporter making filesystem calls itself, (b) the CLI providing multiple sinks, or (c) the reporter writing known absolute paths to the single stream preceded by `##vso[task.uploadsummary]`.
6. **CLI dispatch is flat.** `cmd_lint` constructs the reporter in a single match and writes to `stdout`. There is no options-struct for CI reporters today.
7. **`source_type` is currently unused by any reporter.** `Human` reads it zero times; `Json` surfaces it; `CiGitHub` / `CiAzure` drop it.

## Historical Context (from `research/` and `specs/`)

The canonical design doc for the four-reporter system is [`specs/2026-04-03-lint-display-ux.md`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/specs/2026-04-03-lint-display-ux.md) (Issue #198), backed by [`research/tickets/2026-04-03-198-lint-display-ux.md`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/research/tickets/2026-04-03-198-lint-display-ux.md). Relevant verbatim from the spec:

> `--reporter` flag (values: `human`, `json`, `ci-github`, `ci-azure`) replaces `--format`

> `##vso[task.logissue type={severity};sourcepath={file_path};linenumber={line};columnnumber={col}]{rule_id}: {message}`

And from the research ticket's audit of the original state:

> **Missing fields for Issue #198**: no `column`, no `end_line`, no `end_col`, no `source_snippet`, no `help_url`, no `help_text`.

The Issue #198 spec explicitly rejected SARIF and HTML reporters for v1. Since that work landed, `help_text`, `help_url`, `end_line`, `end_col`, and `col` all became first-class fields of `Diagnostic`. This research is the first to inventory how `CiAzure` compares to `Human` on those newer fields.

Other relevant research:

- [`research/docs/2026-04-10-377-vscode-support-aipm-lint.md`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/research/docs/2026-04-10-377-vscode-support-aipm-lint.md) — documents the four-reporter split and the full `Diagnostic` shape used by LSP.
- [`specs/2026-04-10-vscode-aipm-lint-integration.md`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/specs/2026-04-10-vscode-aipm-lint-integration.md) — LSP mapping uses `help_url → code_description.href` and `rule_id → code (NumberOrString::String)`, showing the same two fields CiAzure drops are the ones LSP clients surface as "Show rule documentation."
- [`research/tickets/2026-04-11-426-dogfood-aipm-lint.md`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/research/tickets/2026-04-11-426-dogfood-aipm-lint.md) and [`specs/2026-04-12-dogfood-aipm-lint.md`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/specs/2026-04-12-dogfood-aipm-lint.md) — the project self-CI uses `--reporter ci-github` (not `ci-azure`); there is no existing ADO pipeline in-repo to dogfood against.
- [`research/docs/2026-04-06-feature-status-audit.md`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/research/docs/2026-04-06-feature-status-audit.md) — lists Issue 205 "Lint: new AI reporter (auto-enable)" as a separate enhancement.
- [`research/docs/2026-04-07-lint-rules-287-288-289-290.md`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/research/docs/2026-04-07-lint-rules-287-288-289-290.md) — records that `Rule::help_url() / help_text()` default to `None` but are universally overridden by shipping rules.
- [`research/docs/2026-04-19-aipm-toml-editor-experience.md`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/research/docs/2026-04-19-aipm-toml-editor-experience.md) — the most recent LSP-adjacent research (uncommitted); relies on the same `Diagnostic` surface.

## Related Research

- [`research/tickets/2026-04-03-198-lint-display-ux.md`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/research/tickets/2026-04-03-198-lint-display-ux.md) — original audit that led to the four-reporter design.
- [`specs/2026-04-03-lint-display-ux.md`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/specs/2026-04-03-lint-display-ux.md) — canonical spec for `--reporter`, `ci-azure` format, color handling.
- [`research/docs/2026-04-10-377-vscode-support-aipm-lint.md`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/research/docs/2026-04-10-377-vscode-support-aipm-lint.md) — LSP / VSCode extension surface analysis.
- [`specs/2026-04-10-vscode-aipm-lint-integration.md`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/specs/2026-04-10-vscode-aipm-lint-integration.md) — LSP mapping of `Diagnostic` fields.
- [`research/tickets/2026-04-11-426-dogfood-aipm-lint.md`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/research/tickets/2026-04-11-426-dogfood-aipm-lint.md), [`specs/2026-04-12-dogfood-aipm-lint.md`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/specs/2026-04-12-dogfood-aipm-lint.md) — self-CI patterns.
- [`specs/2026-04-11-lint-instructions-oversized.md`](https://github.com/TheLarkInn/aipm/blob/3b340745e9c06ab0a2b37d2809ddc470cb2f8916/specs/2026-04-11-lint-instructions-oversized.md) — reporter pipeline integration for a newer rule.

## External References

- [Logging commands — Azure Pipelines | Microsoft Learn](https://learn.microsoft.com/en-us/azure/devops/pipelines/scripts/logging-commands?view=azure-devops) — authoritative spec for all `##vso[...]` and `##[...]` commands used here.
- [azure-pipelines-agent / CommandStringConvertor.cs](https://github.com/microsoft/azure-pipelines-agent/blob/master/src/Agent.Sdk/CommandStringConvertor.cs) — canonical list of recognized parameters per command.
- [percent encoding design](https://github.com/microsoft/azure-pipelines-agent/blob/master/docs/design/percentEncoding.md) — `%AZP25` sentinel rationale.
- [SARIF SAST Scans Tab — Marketplace](https://marketplace.visualstudio.com/items?itemName=sariftools.scans) + [microsoft/sarif-azuredevops-extension](https://github.com/microsoft/sarif-azuredevops-extension).
- [AdvancedSecurity-Publish@1 task reference](https://learn.microsoft.com/en-us/azure/devops/pipelines/tasks/reference/advanced-security-publish-v1?view=azure-pipelines) + [GHAzDO third-party ingestion](https://learn.microsoft.com/en-us/azure/devops/repos/security/github-advanced-security-code-scanning-third-party?view=azure-devops).
- [Configure MSDO extension](https://learn.microsoft.com/en-us/azure/defender-for-cloud/configure-azure-devops-extension) + [Defender for Cloud PR annotations](https://learn.microsoft.com/en-us/azure/defender-for-cloud/enable-pull-request-annotations).
- [Pull Request Threads REST API](https://learn.microsoft.com/en-us/rest/api/azure/devops/git/pull-request-threads).
- [Build Attachments REST API](https://learn.microsoft.com/en-us/rest/api/azure/devops/build/attachments/list).
- [SARIF validator (Azure DevOps mode)](https://sarifweb.azurewebsites.net/Validation).
- Tool precedents: [eslint-formatter-azure-devops (npm)](https://www.npmjs.com/package/eslint-formatter-azure-devops), [EngageSoftware/eslint-formatter-azure-devops](https://github.com/EngageSoftware/eslint-formatter-azure-devops), [PSRule analysis output](https://microsoft.github.io/PSRule/v2/analysis-output/), [melix-dev/azure-devops-dotnet-warnings](https://github.com/melix-dev/azure-devops-dotnet-warnings), [MegaLinter with Azure DevOps](https://aammir-mirza.medium.com/megalinter-with-azure-devops-e88526db7783).
- PR-annotation community patterns: [Thomas Thornton](https://thomasthornton.cloud/2024/01/18/adding-pull-request-comments-to-azure-devops-repo-from-azure-devops-pipelines/), [Peter Moorey](https://petermoorey.github.io/adding-azure-pipeline-comments/), [Cloudlumberjack](https://cloudlumberjack.com/posts/ado-pr-psscriptanalyzer/).
- ANSI rendering: [Developer Community idea](https://developercommunity.visualstudio.com/idea/365961/support-ansi-escape-codes-for-color-in-build-outpu.html), [agent #1569](https://github.com/microsoft/azure-pipelines-agent/issues/1569), [Paul Mackinnon — Medium](https://medium.com/@paul-mackinnon/azure-devops-ansi-colour-coding-df8ef7406422).

## Open Questions

1. **Where should `help_text` / `help_url` go inside `##vso[task.logissue]`?** Two ADO-native landing spots exist: concatenate into the message body (e.g. `<rule_id>: <message> — <help_text> (see <help_url>)`) or populate the unused `code=<rule_id>` property. Microsoft Learn is silent on what the Issues tab does with `code` when `logissue` text contains `(see <url>)` fragments — worth empirical verification inside a real ADO pipeline before committing.
2. **Is `Fs` access acceptable from a CI reporter?** `Human` already holds `&'a dyn Fs` and `base_dir`. Extending `CiAzure` with the same would enable on-disk snippet generation to feed `uploadsummary` markdown, but would change its constructor signature.
3. **Single-stream vs multi-artifact.** The current `Reporter::report(&self, &Outcome, &mut dyn Write)` contract cannot directly write sibling files (`summary.md`, `diagnostics.jsonl`, `results.sarif`). Any richer ADO reporter has to either (a) write those files through `Fs` before emitting `##vso[task.uploadsummary]<path>`, (b) extend the reporter trait with a multi-sink API, or (c) move the responsibility to `cmd_lint`. Deciding the shape of the contract is a prerequisite for richer ADO surfacing.
4. **Does ADO auto-linkify URLs inside `logissue` message bodies?** The modern log viewer linkifies bare URLs, but the Issues tab's rendering of message bodies is undocumented; a concatenated `help_url` may or may not be clickable there.
5. **`end_line` / `end_col` coverage by rules.** Currently no shipping rule sets `end_line` (0 populate); `end_col` usage was not surveyed in this research. Full parity with `Human`'s column-range caret would require rule-side plumbing regardless of reporter output surface — orthogonal to the ADO question.
6. **`source_type` at the ADO layer.** Neither `Human` nor `CiAzure` surfaces `source_type` today; it's available in `Diagnostic` and would be a natural grouping key for `##[group].ai: ... ##[endgroup]` / `##[group].claude: ...`. No existing consumer depends on it in CI output.
7. **PR annotation pathway.** Whether a richer ADO reporter should also include instructions (or a companion script) for surfacing diagnostics as PR-thread comments is a product decision, not a protocol decision.
8. **BDD coverage.** There are no cucumber scenarios covering any of the four reporters' output shapes. Any parity work should consider whether to add `tests/features/guardrails/*.feature` scenarios for the new ADO output (mirroring what exists for command-level behavior).
