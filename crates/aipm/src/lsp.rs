//! LSP server for `aipm lsp` — publishes `aipm lint` diagnostics into VS Code.
//!
//! Uses the stdio transport; start with `aipm lsp` and configure
//! the VS Code extension to launch it as an external language server.
//!
//! Capabilities advertised:
//! - `textDocument/publishDiagnostics` (on open and save)
//! - `textDocument/completion` (rule IDs and severity values in `aipm.toml`)
//! - `textDocument/hover` (rule documentation in `aipm.toml`)

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tower_lsp::jsonrpc;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse,
    Diagnostic as LspDiagnostic, DiagnosticSeverity, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, Documentation, Hover, HoverContents,
    HoverParams, HoverProviderCapability, InitializeParams, InitializeResult, MarkupContent,
    MarkupKind, NumberOrString, Position, Range, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextDocumentSyncOptions, TextDocumentSyncSaveOptions, Url,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};

use libaipm::lint::Severity;

// ── Rule registry ────────────────────────────────────────────────────────────

/// Static metadata about one lint rule (no filesystem interaction).
struct RuleInfo {
    name: &'static str,
    default_severity: Severity,
    help_text: Option<&'static str>,
    help_url: Option<&'static str>,
}

/// Build a rule-ID → `RuleInfo` lookup table from all quality rules.
fn build_rule_index() -> HashMap<String, RuleInfo> {
    libaipm::lint::rules::catalog()
        .into_iter()
        .map(|r| {
            let id = r.id().to_string();
            let info = RuleInfo {
                name: r.name(),
                default_severity: r.default_severity(),
                help_text: r.help_text(),
                help_url: r.help_url(),
            };
            (id, info)
        })
        .collect()
}

// ── Backend ───────────────────────────────────────────────────────────────────

/// Debounce delay before triggering a re-lint on save.
const DEBOUNCE_DELAY: Duration = Duration::from_millis(300);

/// LSP backend — one instance shared across all requests.
pub struct Backend {
    client: Client,
    /// Rule-ID → metadata, used for completions and hover.
    rule_index: HashMap<String, RuleInfo>,
    /// Pending debounce tasks keyed by URI string.
    /// Cancels the previous handle when a new save arrives for the same file.
    debounce_handles: tokio::sync::Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            rule_index: build_rule_index(),
            debounce_handles: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Lint immediately and publish diagnostics for `uri` (no debounce).
    async fn lint_and_publish(&self, uri: Url) {
        let Ok(path) = uri.to_file_path() else {
            tracing::warn!(uri = %uri, "LSP URI is not a file path — skipping lint");
            return;
        };
        let diagnostics =
            tokio::task::spawn_blocking(move || lint_file_diagnostics(&path))
                .await
                .unwrap_or_default();
        self.client.publish_diagnostics(uri, diagnostics, None).await;
    }

    /// Schedule a debounced lint+publish for `uri`.
    ///
    /// Cancels any pending lint for the same URI, then waits [`DEBOUNCE_DELAY`]
    /// before running lint. Rapid saves only trigger one lint pass.
    async fn lint_and_publish_debounced(&self, uri: Url) {
        let key = uri.to_string();

        // Cancel any in-flight debounce for this URI.
        {
            let mut handles = self.debounce_handles.lock().await;
            if let Some(old) = handles.remove(&key) {
                old.abort();
            }
        }

        let client = self.client.clone();
        let uri_clone = uri.clone();

        let handle = tokio::spawn(async move {
            tokio::time::sleep(DEBOUNCE_DELAY).await;
            let Ok(path) = uri_clone.to_file_path() else {
                tracing::warn!(uri = %uri_clone, "LSP URI is not a file path — skipping lint");
                return;
            };
            let diagnostics =
                tokio::task::spawn_blocking(move || lint_file_diagnostics(&path))
                    .await
                    .unwrap_or_default();
            client.publish_diagnostics(uri_clone, diagnostics, None).await;
        });

        let mut handles = self.debounce_handles.lock().await;
        handles.insert(key, handle);
    }
}

// ── LanguageServer impl ───────────────────────────────────────────────────────

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> jsonrpc::Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::NONE),
                        save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                        ..TextDocumentSyncOptions::default()
                    },
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "\"".to_string(),
                        "=".to_string(),
                        " ".to_string(),
                    ]),
                    ..CompletionOptions::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..ServerCapabilities::default()
            },
            ..InitializeResult::default()
        })
    }

    async fn shutdown(&self) -> jsonrpc::Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.lint_and_publish(params.text_document.uri).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.lint_and_publish_debounced(params.text_document.uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.client.publish_diagnostics(params.text_document.uri, vec![], None).await;
    }

    async fn completion(
        &self,
        params: CompletionParams,
    ) -> jsonrpc::Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        // Only offer completions in `aipm.toml` files.
        if !uri.path().ends_with("aipm.toml") {
            return Ok(None);
        }
        let Ok(path) = uri.to_file_path() else {
            return Ok(None);
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Ok(None);
        };

        let cursor_line = params.text_document_position.position.line;
        let items = match detect_completion_context(&text, cursor_line) {
            CompletionCtx::Outside => return Ok(None),
            CompletionCtx::Key => {
                // Offer all rule IDs plus object sub-keys.
                let mut items: Vec<CompletionItem> = self
                    .rule_index
                    .iter()
                    .map(|(id, info)| {
                        let detail = format!(
                            "{} ({})",
                            info.name,
                            match info.default_severity {
                                Severity::Error => "error",
                                Severity::Warning => "warn",
                            }
                        );
                        CompletionItem {
                            label: format!("\"{id}\""),
                            kind: Some(CompletionItemKind::PROPERTY),
                            detail: Some(detail),
                            documentation: info.help_text.map(|t| {
                                Documentation::MarkupContent(MarkupContent {
                                    kind: MarkupKind::Markdown,
                                    value: t.to_string(),
                                })
                            }),
                            insert_text: Some(format!("\"{id}\"")),
                            ..CompletionItem::default()
                        }
                    })
                    .collect();
                // Also offer `level` and `ignore` (used in inline table form).
                items.push(CompletionItem {
                    label: "level".to_string(),
                    kind: Some(CompletionItemKind::FIELD),
                    detail: Some("override severity level".to_string()),
                    ..CompletionItem::default()
                });
                items.push(CompletionItem {
                    label: "ignore".to_string(),
                    kind: Some(CompletionItemKind::FIELD),
                    detail: Some("list of file glob patterns to ignore".to_string()),
                    ..CompletionItem::default()
                });
                items
            },
            CompletionCtx::Value => severity_completions(),
        };

        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn hover(&self, params: HoverParams) -> jsonrpc::Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        if !uri.path().ends_with("aipm.toml") {
            return Ok(None);
        }
        let Ok(path) = uri.to_file_path() else {
            return Ok(None);
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Ok(None);
        };

        let pos = params.text_document_position_params.position;
        let Some(word) = extract_rule_id_at(&text, pos.line, pos.character) else {
            return Ok(None);
        };
        let Some(info) = self.rule_index.get(&word) else {
            return Ok(None);
        };

        let severity_str = match info.default_severity {
            Severity::Error => "error",
            Severity::Warning => "warn",
        };
        let mut md = format!("**{}** (`{word}`)\n\nDefault severity: `{severity_str}`", info.name);
        if let Some(help) = info.help_text {
            md.push_str("\n\n");
            md.push_str(help);
        }
        if let Some(url) = info.help_url {
            md.push_str("\n\n[Documentation](");
            md.push_str(url);
            md.push(')');
        }

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value: md }),
            range: None,
        }))
    }
}

// ── Completion helpers ────────────────────────────────────────────────────────

/// Context for the cursor position in an `aipm.toml` file.
enum CompletionCtx {
    /// Outside `[workspace.lints]` — no completions.
    Outside,
    /// At a key position inside `[workspace.lints]`.
    Key,
    /// At a value position (after `=`) inside `[workspace.lints]`.
    Value,
}

/// Detect the completion context at `cursor_line` within TOML `text`.
fn detect_completion_context(text: &str, cursor_line: u32) -> CompletionCtx {
    let lines: Vec<&str> = text.lines().collect();
    let idx = cursor_line as usize;

    // Walk backwards to find the most recent section header.
    let mut in_workspace_lints = false;
    for i in (0..=idx.min(lines.len().saturating_sub(1))).rev() {
        let trimmed = lines.get(i).copied().unwrap_or("").trim();
        if trimmed == "[workspace.lints]" {
            in_workspace_lints = true;
            break;
        }
        if trimmed.starts_with('[') {
            break; // A different section — stop looking.
        }
    }

    if !in_workspace_lints {
        return CompletionCtx::Outside;
    }

    let current = lines.get(idx).copied().unwrap_or("");
    if current.contains('=') {
        CompletionCtx::Value
    } else {
        CompletionCtx::Key
    }
}

/// Build the fixed list of severity value `CompletionItem`s.
fn severity_completions() -> Vec<CompletionItem> {
    ["allow", "warn", "warning", "error", "deny"]
        .iter()
        .map(|s| CompletionItem {
            label: format!("\"{s}\""),
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            insert_text: Some(format!("\"{s}\"")),
            ..CompletionItem::default()
        })
        .collect()
}

// ── Hover helpers ─────────────────────────────────────────────────────────────

/// Extract the rule ID (or bare word) at `(line, character)` in `text`.
///
/// A rule-ID character is alphanumeric, `/`, `-`, or `_`.
fn extract_rule_id_at(text: &str, line: u32, character: u32) -> Option<String> {
    let line_text = text.lines().nth(line as usize)?;
    let chars: Vec<char> = line_text.chars().collect();
    let pos = character as usize;

    let is_rule_char = |c: char| c.is_alphanumeric() || c == '/' || c == '-' || c == '_';

    if !chars.get(pos).copied().is_some_and(is_rule_char) {
        return None;
    }

    let mut start = pos;
    while start > 0 && chars.get(start - 1).copied().is_some_and(is_rule_char) {
        start -= 1;
    }

    let mut end = pos;
    while chars.get(end + 1).copied().is_some_and(is_rule_char) {
        end += 1;
    }

    let word: String = chars.get(start..=end)?.iter().collect();
    if word.is_empty() { None } else { Some(word) }
}

// ── Lint helper ───────────────────────────────────────────────────────────────

/// Walk up from `path` looking for a workspace root marker (`aipm.toml` or `.ai/`).
fn find_workspace_dir(path: &Path) -> PathBuf {
    let start = if path.is_file() {
        path.parent().map_or_else(|| path.to_path_buf(), Path::to_path_buf)
    } else {
        path.to_path_buf()
    };

    let mut dir = start.clone();
    loop {
        if dir.join("aipm.toml").exists() || dir.join(".ai").exists() {
            return dir;
        }
        match dir.parent() {
            Some(parent) if parent != dir => dir = parent.to_path_buf(),
            _ => break,
        }
    }

    start
}

/// Convert one `libaipm` diagnostic to an LSP diagnostic.
fn to_lsp_diagnostic(d: &libaipm::lint::Diagnostic) -> LspDiagnostic {
    // aipm uses 1-based lines/cols; LSP uses 0-based.
    let start_line = d.line.map_or(0, |l| l.saturating_sub(1));
    let start_char = d.col.map_or(0, |c| c.saturating_sub(1));
    let end_line = d.end_line.map_or(start_line, |l| l.saturating_sub(1));
    // end_col is exclusive in aipm; LSP end character is also exclusive — no adjustment needed.
    let end_char = d.end_col.map_or_else(|| start_char.saturating_add(1), |c| c.saturating_sub(1));

    let range = Range {
        start: Position {
            line: u32::try_from(start_line).unwrap_or(u32::MAX),
            character: u32::try_from(start_char).unwrap_or(u32::MAX),
        },
        end: Position {
            line: u32::try_from(end_line).unwrap_or(u32::MAX),
            character: u32::try_from(end_char).unwrap_or(u32::MAX),
        },
    };

    let severity = match d.severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
    };

    LspDiagnostic {
        range,
        severity: Some(severity),
        code: Some(NumberOrString::String(d.rule_id.clone())),
        source: Some("aipm".to_string()),
        message: d.message.clone(),
        code_description: None,
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Run `aipm lint` for the workspace containing `file_path` and return
/// LSP diagnostics that belong to `file_path`.
fn lint_file_diagnostics(file_path: &Path) -> Vec<LspDiagnostic> {
    let workspace_dir = find_workspace_dir(file_path);
    let config = crate::load_lint_config(&workspace_dir);
    let opts =
        libaipm::lint::Options { dir: workspace_dir, source: None, config, max_depth: None };

    match libaipm::lint::lint(&opts, &libaipm::fs::Real) {
        Ok(outcome) => outcome
            .diagnostics
            .iter()
            .filter(|d| d.file_path == file_path)
            .map(to_lsp_diagnostic)
            .collect(),
        Err(e) => {
            tracing::warn!(error = %e, "lint failed during LSP file check");
            vec![]
        },
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Entry point for `aipm lsp` — blocks until the client disconnects.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    rt.block_on(async {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let (service, socket) = LspService::new(Backend::new);
        Server::new(stdin, stdout, socket).serve(service).await;
    });
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── find_workspace_dir ────────────────────────────────────────────────────

    #[test]
    fn workspace_dir_with_ai_marker() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join(".ai")).expect("mkdir .ai");
        let file = dir.path().join(".ai/test.md");
        std::fs::write(&file, "").expect("write");
        assert_eq!(find_workspace_dir(&file), dir.path());
    }

    #[test]
    fn workspace_dir_with_manifest() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("aipm.toml"), "").expect("write");
        let sub = dir.path().join("sub");
        std::fs::create_dir_all(&sub).expect("mkdir");
        let file = sub.join("skill.md");
        std::fs::write(&file, "").expect("write");
        assert_eq!(find_workspace_dir(&file), dir.path());
    }

    #[test]
    fn workspace_dir_fallback_to_parent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("orphan.md");
        std::fs::write(&file, "").expect("write");
        assert_eq!(find_workspace_dir(&file), dir.path());
    }

    // ── to_lsp_diagnostic ────────────────────────────────────────────────────

    #[test]
    fn lsp_diag_error_severity() {
        let d = libaipm::lint::Diagnostic {
            rule_id: "skill/missing-name".to_string(),
            severity: Severity::Error,
            message: "Missing name".to_string(),
            file_path: PathBuf::from("SKILL.md"),
            line: Some(1),
            col: Some(1),
            end_line: Some(1),
            end_col: Some(4),
            source_type: ".ai".to_string(),
            help_text: None,
            help_url: None,
        };
        let lsp = to_lsp_diagnostic(&d);
        assert_eq!(lsp.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(lsp.range.start.line, 0);
        assert_eq!(lsp.range.start.character, 0);
        assert_eq!(lsp.range.end.character, 3);
    }

    #[test]
    fn lsp_diag_warning_severity() {
        let d = libaipm::lint::Diagnostic {
            rule_id: "hook/legacy-event-name".to_string(),
            severity: Severity::Warning,
            message: "legacy".to_string(),
            file_path: PathBuf::from("hooks.json"),
            line: Some(2),
            col: Some(3),
            end_line: Some(2),
            end_col: Some(9),
            source_type: ".ai".to_string(),
            help_text: None,
            help_url: None,
        };
        let lsp = to_lsp_diagnostic(&d);
        assert_eq!(lsp.severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(lsp.range.start.line, 1);
        assert_eq!(lsp.range.start.character, 2);
        assert_eq!(lsp.range.end.character, 8);
    }

    #[test]
    fn lsp_diag_no_position_defaults_to_zero() {
        let d = libaipm::lint::Diagnostic {
            rule_id: "test/rule".to_string(),
            severity: Severity::Warning,
            message: "no position".to_string(),
            file_path: PathBuf::from("file.md"),
            line: None,
            col: None,
            end_line: None,
            end_col: None,
            source_type: ".ai".to_string(),
            help_text: None,
            help_url: None,
        };
        let lsp = to_lsp_diagnostic(&d);
        assert_eq!(lsp.range.start.line, 0);
        assert_eq!(lsp.range.start.character, 0);
        assert_eq!(lsp.range.end.character, 1);
    }

    #[test]
    fn lsp_diag_rule_id_as_code() {
        let d = libaipm::lint::Diagnostic {
            rule_id: "skill/name-too-long".to_string(),
            severity: Severity::Error,
            message: "too long".to_string(),
            file_path: PathBuf::from("SKILL.md"),
            line: None,
            col: None,
            end_line: None,
            end_col: None,
            source_type: ".ai".to_string(),
            help_text: None,
            help_url: None,
        };
        let lsp = to_lsp_diagnostic(&d);
        assert_eq!(lsp.code, Some(NumberOrString::String("skill/name-too-long".to_string())));
        assert_eq!(lsp.source, Some("aipm".to_string()));
    }

    // ── detect_completion_context ─────────────────────────────────────────────

    #[test]
    fn context_outside_section_is_outside() {
        let text = "[package]\nname = \"foo\"\n";
        assert!(matches!(detect_completion_context(text, 1), CompletionCtx::Outside));
    }

    #[test]
    fn context_inside_workspace_lints_no_equals_is_key() {
        let text = "[workspace.lints]\n\"skill/missing-name\"\n";
        assert!(matches!(detect_completion_context(text, 1), CompletionCtx::Key));
    }

    #[test]
    fn context_inside_workspace_lints_with_equals_is_value() {
        let text = "[workspace.lints]\n\"skill/missing-name\" = \n";
        assert!(matches!(detect_completion_context(text, 1), CompletionCtx::Value));
    }

    #[test]
    fn context_different_section_after_workspace_lints_is_outside() {
        let text = "[workspace.lints]\n\"skill/missing-name\" = \"warn\"\n[other]\nfoo = 1\n";
        assert!(matches!(detect_completion_context(text, 3), CompletionCtx::Outside));
    }

    #[test]
    fn context_empty_document_is_outside() {
        assert!(matches!(detect_completion_context("", 0), CompletionCtx::Outside));
    }

    // ── extract_rule_id_at ────────────────────────────────────────────────────

    #[test]
    fn extract_rule_id_within_quoted_string() {
        // `"skill/missing-name" = "warn"`
        // cursor at col 7 (inside "skill/missing-name")
        let text = "\"skill/missing-name\" = \"warn\"\n";
        let result = extract_rule_id_at(text, 0, 7);
        assert_eq!(result.as_deref(), Some("skill/missing-name"));
    }

    #[test]
    fn extract_rule_id_at_start_of_word() {
        let text = "\"hook/unknown-event\" = \"error\"\n";
        let result = extract_rule_id_at(text, 0, 1);
        assert_eq!(result.as_deref(), Some("hook/unknown-event"));
    }

    #[test]
    fn extract_rule_id_at_non_rule_char_returns_none() {
        let text = "\"skill/missing-name\" = \"warn\"\n";
        // position 0 is `"` which is not a rule char
        assert!(extract_rule_id_at(text, 0, 0).is_none());
    }

    #[test]
    fn extract_rule_id_past_end_of_line_returns_none() {
        let text = "abc\n";
        assert!(extract_rule_id_at(text, 0, 100).is_none());
    }

    #[test]
    fn extract_rule_id_nonexistent_line_returns_none() {
        let text = "abc\n";
        assert!(extract_rule_id_at(text, 5, 0).is_none());
    }

    // ── severity_completions ──────────────────────────────────────────────────

    #[test]
    fn severity_completions_has_all_values() {
        let items = severity_completions();
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"\"allow\""));
        assert!(labels.contains(&"\"warn\""));
        assert!(labels.contains(&"\"error\""));
        assert!(labels.contains(&"\"deny\""));
        assert!(labels.contains(&"\"warning\""));
    }

    // ── build_rule_index ──────────────────────────────────────────────────────

    #[test]
    fn rule_index_contains_all_quality_rules() {
        let index = build_rule_index();
        assert!(index.contains_key("skill/missing-name"));
        assert!(index.contains_key("hook/unknown-event"));
        assert!(index.contains_key("agent/missing-tools"));
        assert!(index.contains_key("plugin/missing-manifest"));
        // 16 quality rules total
        assert_eq!(index.len(), 16);
    }

    #[test]
    fn rule_index_entry_has_correct_name() {
        let index = build_rule_index();
        let info = index.get("skill/missing-name").expect("rule exists");
        assert_eq!(info.name, "missing skill name");
    }

    // ── lint_file_diagnostics — all 6 feature kinds ───────────────────────────

    /// Build a minimal workspace with `.ai/` marker and one feature file.
    /// Returns `(TempDir, absolute_path_to_feature_file)`.
    fn make_workspace(
        rel_path: &str,
        content: &str,
    ) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join(rel_path);
        std::fs::create_dir_all(file.parent().expect("parent")).expect("mkdir");
        std::fs::write(&file, content).expect("write");
        // Create .ai/ marker so find_workspace_dir() recognises this as a workspace root.
        std::fs::create_dir_all(dir.path().join(".ai")).expect("mkdir .ai");
        (dir, file)
    }

    #[test]
    fn lint_diagnostics_skill_name_invalid() {
        let (_dir, file) = make_workspace(
            ".ai/p/skills/default/SKILL.md",
            "---\nname: inv@lid!\ndescription: A skill\n---\n",
        );
        let diags = lint_file_diagnostics(&file);
        assert!(
            diags.iter().any(|d| d.code == Some(NumberOrString::String("skill/name-invalid-chars".to_string()))),
            "expected skill/name-invalid-chars diagnostic, got: {diags:?}",
        );
    }

    #[test]
    fn lint_diagnostics_agent_missing_tools() {
        let (_dir, file) = make_workspace(
            ".ai/p/agents/reviewer.md",
            "---\n---\n",
        );
        let diags = lint_file_diagnostics(&file);
        assert!(
            diags.iter().any(|d| d.code == Some(NumberOrString::String("agent/missing-tools".to_string()))),
            "expected agent/missing-tools diagnostic, got: {diags:?}",
        );
    }

    #[test]
    fn lint_diagnostics_hook_unknown_event() {
        let (_dir, file) = make_workspace(
            ".ai/p/hooks/hooks.json",
            r#"{"UnknownEvent": []}"#,
        );
        let diags = lint_file_diagnostics(&file);
        assert!(
            diags.iter().any(|d| d.code == Some(NumberOrString::String("hook/unknown-event".to_string()))),
            "expected hook/unknown-event diagnostic, got: {diags:?}",
        );
    }

    #[test]
    fn lint_diagnostics_plugin_json_missing_required_field() {
        let (_dir, file) = make_workspace(
            ".ai/p/.claude-plugin/plugin.json",
            r#"{"name": "test-plugin"}"#,
        );
        let diags = lint_file_diagnostics(&file);
        assert!(
            diags.iter().any(|d| d.code == Some(NumberOrString::String("plugin/required-fields".to_string()))),
            "expected plugin/required-fields diagnostic, got: {diags:?}",
        );
    }

    #[test]
    fn lint_diagnostics_skill_clean_returns_no_diagnostics_for_file() {
        let (_dir, file) = make_workspace(
            ".ai/p/skills/default/SKILL.md",
            "---\nname: my-skill\ndescription: A valid description\n---\nBody\n",
        );
        let diags = lint_file_diagnostics(&file);
        assert!(
            diags.is_empty(),
            "expected no diagnostics for clean skill, got: {diags:?}",
        );
    }

    #[test]
    fn lint_diagnostics_stale_clearing_clean_file_returns_empty() {
        // Simulates the "stale clearing" scenario: a file that was fixed should
        // produce empty diagnostics (which clears the VS Code markers).
        let (_dir, file) = make_workspace(
            ".ai/p/agents/coder.md",
            "---\ntools: All\n---\n",
        );
        let diags = lint_file_diagnostics(&file);
        assert!(diags.is_empty(), "clean agent file should have no diagnostics");
    }
}
