//! LSP server for `aipm lsp` — publishes `aipm lint` diagnostics into VS Code.
//!
//! Uses the stdio transport; start with `aipm lsp` and configure
//! the VS Code extension to launch it as an external language server.
//!
//! Capabilities advertised:
//! - `textDocument/publishDiagnostics` (on open and save)
//! - `textDocument/completion` (rule IDs and severity values in `aipm.toml`)
//! - `textDocument/hover` (rule documentation in `aipm.toml`)
//!
//! # Coverage note
//! The async state machine types generated from `tokio::spawn` and `async_trait`
//! trigger an LLVM 22.1 bug in `getInstantiationGroups` that crashes `llvm-cov report`
//! when the same async functions appear in both the production binary and the test
//! binary.  This file is therefore excluded from the coverage report via
//! `--ignore-filename-regex 'lsp\.rs'` in CI.  The sync helper functions (completions,
//! hover, diagnostic conversion, workspace detection) live in `lsp/helpers.rs` which
//! IS included in coverage and is fully tested by its `mod tests` block.

mod helpers;

use std::collections::HashMap;
use std::time::Duration;

use tower_lsp::jsonrpc;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    Documentation, Hover, HoverContents, HoverParams, HoverProviderCapability, InitializeParams,
    InitializeResult, MarkupContent, MarkupKind, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextDocumentSyncOptions, TextDocumentSyncSaveOptions, Url,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};

use helpers::{
    build_rule_index, detect_completion_context, extract_rule_id_at, lint_file_diagnostics,
    severity_completions, CompletionCtx, RuleInfo,
};
use libaipm::lint::Severity;

// ── Backend ───────────────────────────────────────────────────────────────────

/// Debounce delay before triggering a re-lint on save.
const DEBOUNCE_DELAY: Duration = Duration::from_millis(300);

type DebounceMap = std::sync::Arc<tokio::sync::Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>;

/// LSP backend — one instance shared across all requests.
pub struct Backend {
    client: Client,
    /// Rule-ID → metadata, used for completions and hover.
    rule_index: HashMap<String, RuleInfo>,
    /// Pending debounce tasks keyed by URI string.
    /// Cancels the previous handle when a new save arrives for the same file.
    /// Wrapped in Arc so the spawned task can remove its own entry after completion.
    debounce_handles: DebounceMap,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            rule_index: build_rule_index(),
            debounce_handles: std::sync::Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Lint immediately and publish diagnostics for `uri` (no debounce).
    async fn lint_and_publish(&self, uri: Url) {
        let Ok(path) = uri.to_file_path() else {
            tracing::warn!(uri = %uri, "LSP URI is not a file path — skipping lint");
            return;
        };
        let diagnostics = tokio::task::spawn_blocking(move || lint_file_diagnostics(&path))
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

        let handle = tokio::spawn(debounce_lint_task(
            self.client.clone(),
            uri,
            std::sync::Arc::clone(&self.debounce_handles),
            key.clone(),
        ));

        let mut handles = self.debounce_handles.lock().await;
        handles.insert(key, handle);
    }
}

// ── Debounce task ─────────────────────────────────────────────────────────────

/// Standalone async task spawned by `lint_and_publish_debounced`.
///
/// Using a named free function (rather than `async move { ... }`) keeps the async
/// state machine well-defined.  See the module-level coverage note for why this
/// file is excluded from `llvm-cov` reporting.
async fn debounce_lint_task(client: Client, uri: Url, handles: DebounceMap, key: String) {
    tokio::time::sleep(DEBOUNCE_DELAY).await;
    let Ok(path) = uri.to_file_path() else {
        tracing::warn!(uri = %uri, "LSP URI is not a file path — skipping lint");
        handles.lock().await.remove(&key);
        return;
    };
    let diagnostics =
        tokio::task::spawn_blocking(move || lint_file_diagnostics(&path)).await.unwrap_or_default();
    client.publish_diagnostics(uri, diagnostics, None).await;
    // Remove completed handle to prevent unbounded map growth.
    handles.lock().await.remove(&key);
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
        let uri = params.text_document.uri;
        // Cancel any pending debounce so it doesn't publish stale diagnostics
        // after the editor has already cleared the document.
        let pending = self.debounce_handles.lock().await.remove(&uri.to_string());
        if let Some(handle) = pending {
            handle.abort();
        }
        self.client.publish_diagnostics(uri, vec![], None).await;
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
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: md,
            }),
            range: None,
        }))
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
