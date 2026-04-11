//! Synchronous helper functions and types for the `aipm` LSP server.
//!
//! This module contains the pure sync functions used by the LSP backend:
//! rule index construction, completion context detection, hover extraction,
//! diagnostic conversion, and workspace root discovery.  These are kept
//! separate from the async `Backend` implementation in `lsp.rs` so that
//! llvm-cov can instrument their branch coverage without hitting the LLVM
//! 22.1 `getInstantiationGroups` crash that affects async state machines.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tower_lsp::lsp_types::{
    CodeDescription, CompletionItem, CompletionItemKind, Diagnostic as LspDiagnostic,
    DiagnosticSeverity, NumberOrString, Position, Range, Url,
};

use libaipm::lint::Severity;

// ── Rule registry ─────────────────────────────────────────────────────────────

/// Static metadata about one lint rule (no filesystem interaction).
pub(super) struct RuleInfo {
    pub(super) name: &'static str,
    pub(super) default_severity: Severity,
    pub(super) help_text: Option<&'static str>,
    pub(super) help_url: Option<&'static str>,
}

/// Build a rule-ID → `RuleInfo` lookup table from all quality rules.
pub(super) fn build_rule_index() -> HashMap<String, RuleInfo> {
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

// ── Completion helpers ────────────────────────────────────────────────────────

/// Context for the cursor position in an `aipm.toml` file.
pub(super) enum CompletionCtx {
    /// Outside `[workspace.lints]` — no completions.
    Outside,
    /// At a key position inside `[workspace.lints]`.
    Key,
    /// At a value position (after `=`) inside `[workspace.lints]`.
    Value,
}

/// Detect the completion context at `cursor_line` within TOML `text`.
pub(super) fn detect_completion_context(text: &str, cursor_line: u32) -> CompletionCtx {
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
pub(super) fn severity_completions() -> Vec<CompletionItem> {
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
pub(super) fn extract_rule_id_at(text: &str, line: u32, character: u32) -> Option<String> {
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
    if word.is_empty() {
        None
    } else {
        Some(word)
    }
}

// ── Lint helper ───────────────────────────────────────────────────────────────

/// Walk up from `path`'s parent directory (when available) looking for a
/// workspace root marker (`aipm.toml` or `.ai/`).
///
/// Always starts from the parent so that new or unsaved files (where
/// `path.is_file()` would be `false`) are still resolved correctly.
pub(super) fn find_workspace_dir(path: &Path) -> PathBuf {
    let start = path.parent().map_or_else(|| path.to_path_buf(), Path::to_path_buf);

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
pub(super) fn to_lsp_diagnostic(d: &libaipm::lint::Diagnostic) -> LspDiagnostic {
    // aipm uses 1-based lines/cols; LSP uses 0-based.
    let start_line = d.line.map_or(0, |l| l.saturating_sub(1));
    let start_char = d.col.map_or(0, |c| c.saturating_sub(1));
    let end_line = d.end_line.map_or(start_line, |l| l.saturating_sub(1));
    // end_col is exclusive in aipm and LSP end character is also exclusive, but aipm
    // columns are 1-based while LSP characters are 0-based, so subtract 1 to convert.
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
        code_description: d
            .help_url
            .as_deref()
            .and_then(|u| Url::parse(u).ok().map(|href| CodeDescription { href })),
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Run `aipm lint` for the workspace containing `file_path` and return
/// LSP diagnostics that belong to `file_path`.
pub(super) fn lint_file_diagnostics(file_path: &Path) -> Vec<LspDiagnostic> {
    let workspace_dir = find_workspace_dir(file_path);
    let config = crate::load_lint_config(&workspace_dir);
    let opts = libaipm::lint::Options { dir: workspace_dir, source: None, config, max_depth: None };

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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tower_lsp::lsp_types::{DiagnosticSeverity, NumberOrString};

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

    #[test]
    fn lsp_diag_help_url_populates_code_description() {
        let d = libaipm::lint::Diagnostic {
            rule_id: "skill/name-invalid-chars".to_string(),
            message: "invalid chars".to_string(),
            severity: Severity::Error,
            file_path: std::path::PathBuf::new(),
            line: Some(3),
            col: Some(8),
            end_line: Some(3),
            end_col: Some(14),
            source_type: ".claude".to_string(),
            help_text: None,
            help_url: Some("https://example.com/rules/skill-name-invalid-chars".to_string()),
        };
        let lsp = to_lsp_diagnostic(&d);
        let desc = lsp.code_description.expect("code_description should be set");
        assert_eq!(desc.href.as_str(), "https://example.com/rules/skill-name-invalid-chars");
    }

    #[test]
    fn lsp_diag_no_help_url_leaves_code_description_none() {
        let d = libaipm::lint::Diagnostic {
            rule_id: "skill/oversized".to_string(),
            message: "too big".to_string(),
            severity: Severity::Warning,
            file_path: std::path::PathBuf::new(),
            line: None,
            col: None,
            end_line: None,
            end_col: None,
            source_type: ".claude".to_string(),
            help_text: None,
            help_url: None,
        };
        let lsp = to_lsp_diagnostic(&d);
        assert!(lsp.code_description.is_none());
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

    #[test]
    fn extract_rule_id_word_starts_at_column_zero() {
        // The rule ID starts at the very beginning of the line (column 0).
        // The backwards scan loop at `while start > 0 && ...` must terminate
        // because `start` reaches 0, covering the `start == 0` exit branch.
        let text = "skill/missing-name = \"warn\"\n";
        // Cursor at col 3 (inside "skill"); the scan walks back to col 0.
        let result = extract_rule_id_at(text, 0, 3);
        assert_eq!(result.as_deref(), Some("skill/missing-name"));
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
        assert!(index.contains_key("source/misplaced-features"));
        // 17 rules total (16 quality rules + source/misplaced-features)
        assert_eq!(index.len(), 17);
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
    fn make_workspace(rel_path: &str, content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
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
            diags
                .iter()
                .any(|d| d.code
                    == Some(NumberOrString::String("skill/name-invalid-chars".to_string()))),
            "expected skill/name-invalid-chars diagnostic, got: {diags:?}",
        );
    }

    #[test]
    fn lint_diagnostics_agent_missing_tools() {
        let (_dir, file) = make_workspace(".ai/p/agents/reviewer.md", "---\n---\n");
        let diags = lint_file_diagnostics(&file);
        assert!(
            diags
                .iter()
                .any(|d| d.code == Some(NumberOrString::String("agent/missing-tools".to_string()))),
            "expected agent/missing-tools diagnostic, got: {diags:?}",
        );
    }

    #[test]
    fn lint_diagnostics_hook_unknown_event() {
        let (_dir, file) = make_workspace(".ai/p/hooks/hooks.json", r#"{"UnknownEvent": []}"#);
        let diags = lint_file_diagnostics(&file);
        assert!(
            diags
                .iter()
                .any(|d| d.code == Some(NumberOrString::String("hook/unknown-event".to_string()))),
            "expected hook/unknown-event diagnostic, got: {diags:?}",
        );
    }

    #[test]
    fn lint_diagnostics_plugin_json_missing_required_field() {
        let (_dir, file) =
            make_workspace(".ai/p/.claude-plugin/plugin.json", r#"{"name": "test-plugin"}"#);
        let diags = lint_file_diagnostics(&file);
        assert!(
            diags
                .iter()
                .any(|d| d.code
                    == Some(NumberOrString::String("plugin/required-fields".to_string()))),
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
        assert!(diags.is_empty(), "expected no diagnostics for clean skill, got: {diags:?}",);
    }

    #[test]
    fn lint_diagnostics_stale_clearing_clean_file_returns_empty() {
        // Simulates the "stale clearing" scenario: a file that was fixed should
        // produce empty diagnostics (which clears the VS Code markers).
        let (_dir, file) = make_workspace(".ai/p/agents/coder.md", "---\ntools: All\n---\n");
        let diags = lint_file_diagnostics(&file);
        assert!(diags.is_empty(), "clean agent file should have no diagnostics");
    }
}
