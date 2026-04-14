//! Lint rule implementations and factory functions.
//!
//! The primary entry points are [`quality_rules_for_kind()`] (kind-based dispatch
//! for the unified discovery pipeline) and [`misplaced_features_rule()`] (produces
//! a per-feature `source/misplaced-features` rule instance).

pub mod agent_missing_tools;
pub mod broken_paths;
pub mod hook_legacy_event;
pub mod hook_unknown_event;
pub mod import_resolver;
pub mod instructions_oversized;
pub mod known_events;
pub mod marketplace_field_mismatch;
pub mod marketplace_source_resolve;
pub mod misplaced_features;
pub mod plugin_missing_manifest;
pub mod plugin_missing_registration;
pub mod plugin_required_fields;
pub(crate) mod scan;
pub mod skill_desc_too_long;
pub mod skill_invalid_shell;
pub mod skill_missing_desc;
pub mod skill_missing_name;
pub mod skill_name_invalid;
pub mod skill_name_too_long;
pub mod skill_oversized;
#[cfg(test)]
pub(crate) mod test_helpers;

use std::path::Path;

use crate::discovery::{DiscoveredFeature, FeatureKind};
use crate::lint::diagnostic::{Diagnostic, Severity};
use misplaced_features::MisplacedFeatures;

use super::config::Config;
use super::rule::Rule;

/// Return `(line_num, col, end_col)` for a JSON key in a string of JSON content.
///
/// Searches for the first line containing `"key"` and returns:
/// - `line_num`: 1-based line number
/// - `col`: 1-based column of the opening `"`
/// - `end_col`: 1-based exclusive column past the closing `"`
pub(crate) fn locate_json_key(content: &str, key: &str) -> Option<(usize, usize, usize)> {
    let needle = format!("\"{key}\"");
    for (i, line) in content.lines().enumerate() {
        if let Some(pos) = line.find(&needle) {
            return Some((i + 1, pos + 1, pos + needle.len() + 1));
        }
    }
    None
}

/// Create a simple diagnostic with no line/col information.
///
/// Shared by marketplace and plugin rules that produce diagnostics
/// without precise source positions.
pub(crate) fn simple_diag(
    rule_id: &str,
    severity: Severity,
    message: String,
    file_path: &Path,
    source_type: &str,
) -> Diagnostic {
    Diagnostic {
        rule_id: rule_id.to_string(),
        severity,
        message,
        file_path: file_path.to_path_buf(),
        line: None,
        col: None,
        end_line: None,
        end_col: None,
        source_type: source_type.to_string(),
        help_text: None,
        help_url: None,
    }
}

use crate::fs::Fs;

/// Read a skill file and compute its source type in one call.
///
/// Returns `None` if the file cannot be read, matching the early-return
/// pattern used by all skill lint rules.
pub(crate) fn read_skill_preamble(
    file_path: &Path,
    fs: &dyn Fs,
) -> Option<(String, scan::FoundSkill)> {
    let source_type = scan::source_type_from_path(file_path).to_string();
    let skill = scan::read_skill(file_path, fs)?;
    Some((source_type, skill))
}

/// Read an agent file and compute its source type in one call.
///
/// Returns `None` if the file cannot be read, matching the early-return
/// pattern used by agent lint rules.
pub(crate) fn read_agent_preamble(
    file_path: &Path,
    fs: &dyn Fs,
) -> Option<(String, scan::FoundAgent)> {
    let source_type = scan::source_type_from_path(file_path).to_string();
    let agent = scan::read_agent(file_path, fs)?;
    Some((source_type, agent))
}

/// Read a hook file and compute its source type in one call.
///
/// Returns `None` if the file cannot be read, matching the early-return
/// pattern used by hook lint rules.
pub(crate) fn read_hook_preamble(file_path: &Path, fs: &dyn Fs) -> Option<(String, String)> {
    let source_type = scan::source_type_from_path(file_path).to_string();
    let (_path, content) = scan::read_hook(file_path, fs)?;
    Some((source_type, content))
}

/// Get quality rules applicable to a feature kind.
///
/// These rules validate individual feature files without regard to which
/// source directory the feature came from.
pub(crate) fn quality_rules_for_kind(kind: &FeatureKind, config: &Config) -> Vec<Box<dyn Rule>> {
    match kind {
        FeatureKind::Skill => vec![
            Box::new(skill_missing_name::MissingName),
            Box::new(skill_missing_desc::MissingDescription),
            Box::new(skill_oversized::Oversized),
            Box::new(skill_name_too_long::NameTooLong),
            Box::new(skill_name_invalid::NameInvalidChars),
            Box::new(skill_desc_too_long::DescriptionTooLong),
            Box::new(skill_invalid_shell::InvalidShell),
            Box::new(broken_paths::BrokenPaths),
        ],
        FeatureKind::Agent => vec![Box::new(agent_missing_tools::MissingTools)],
        FeatureKind::Hook => vec![
            Box::new(hook_unknown_event::UnknownEvent),
            Box::new(hook_legacy_event::LegacyEventName),
        ],
        FeatureKind::Plugin => vec![Box::new(broken_paths::BrokenPaths)],
        FeatureKind::Marketplace => vec![
            Box::new(marketplace_source_resolve::SourceResolve),
            Box::new(marketplace_field_mismatch::FieldMismatch),
            Box::new(plugin_missing_registration::MissingRegistration),
            Box::new(plugin_missing_manifest::MissingManifest),
        ],
        FeatureKind::PluginJson => vec![Box::new(plugin_required_fields::RequiredFields)],
        FeatureKind::Instructions => {
            let opts = config.rule_options("instructions/oversized");
            let max_lines = opts
                .get("lines")
                .and_then(toml::Value::as_integer)
                .and_then(|v| usize::try_from(v).ok())
                .unwrap_or(instructions_oversized::DEFAULT_MAX_LINES);
            let max_chars = opts
                .get("characters")
                .and_then(toml::Value::as_integer)
                .and_then(|v| usize::try_from(v).ok())
                .unwrap_or(instructions_oversized::DEFAULT_MAX_CHARS);
            let resolve_imports =
                opts.get("resolve-imports").and_then(toml::Value::as_bool).unwrap_or(false);
            vec![Box::new(instructions_oversized::Oversized {
                max_lines,
                max_chars,
                resolve_imports,
            })]
        },
    }
}

/// Returns every lint rule, including `source/misplaced-features`.
///
/// Useful for tools that need a rule registry (e.g., LSP completions, hover).
/// `source/misplaced-features` is included with `ai_exists: true` so that the
/// hover/completion help text reflects the common case (marketplace already
/// present) — users seeing this in the LSP almost certainly have `.ai/`.
pub fn catalog() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(skill_missing_name::MissingName),
        Box::new(skill_missing_desc::MissingDescription),
        Box::new(skill_oversized::Oversized),
        Box::new(skill_name_too_long::NameTooLong),
        Box::new(skill_name_invalid::NameInvalidChars),
        Box::new(skill_desc_too_long::DescriptionTooLong),
        Box::new(skill_invalid_shell::InvalidShell),
        Box::new(broken_paths::BrokenPaths),
        Box::new(agent_missing_tools::MissingTools),
        Box::new(hook_unknown_event::UnknownEvent),
        Box::new(hook_legacy_event::LegacyEventName),
        Box::new(marketplace_source_resolve::SourceResolve),
        Box::new(marketplace_field_mismatch::FieldMismatch),
        Box::new(plugin_missing_registration::MissingRegistration),
        Box::new(plugin_missing_manifest::MissingManifest),
        Box::new(plugin_required_fields::RequiredFields),
        Box::new(instructions_oversized::Oversized::default()),
        Box::new(MisplacedFeatures { ai_exists: true }),
    ]
}

/// Construct a `MisplacedFeatures` rule instance for a discovered feature.
pub(crate) const fn misplaced_features_rule(
    feature: &DiscoveredFeature,
    ai_exists: bool,
) -> MisplacedFeatures {
    misplaced_features::misplaced_features_rule(feature, ai_exists)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::config::Config;

    #[test]
    fn locate_json_key_finds_key_on_first_line() {
        let content = r#"{"event": []}"#;
        let result = locate_json_key(content, "event");
        // "event" starts at col 2 (after '{'), col+len("\"event\"")=2+7=9
        assert_eq!(result, Some((1, 2, 9)));
    }

    #[test]
    fn locate_json_key_finds_key_on_later_line() {
        let content = "{\n  \"event\": []\n}";
        let result = locate_json_key(content, "event");
        assert_eq!(result, Some((2, 3, 10)));
    }

    #[test]
    fn locate_json_key_returns_none_for_missing_key() {
        let content = r#"{"other": []}"#;
        assert_eq!(locate_json_key(content, "event"), None);
    }

    #[test]
    fn locate_json_key_empty_content() {
        assert_eq!(locate_json_key("", "key"), None);
    }

    #[test]
    fn simple_diag_creates_diagnostic_with_no_positions() {
        let d = simple_diag(
            "test/rule",
            Severity::Error,
            "test message".to_string(),
            Path::new("test.json"),
            ".ai",
        );
        assert_eq!(d.rule_id, "test/rule");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.message, "test message");
        assert_eq!(d.file_path, Path::new("test.json"));
        assert_eq!(d.source_type, ".ai");
        assert_eq!(d.line, None);
        assert_eq!(d.col, None);
        assert_eq!(d.end_line, None);
        assert_eq!(d.end_col, None);
        assert!(d.help_text.is_none());
        assert!(d.help_url.is_none());
    }

    #[test]
    fn simple_diag_warning_severity() {
        let d = simple_diag(
            "test/warn",
            Severity::Warning,
            "warn msg".to_string(),
            Path::new("f.json"),
            ".claude",
        );
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.source_type, ".claude");
    }

    #[test]
    fn read_skill_preamble_returns_some_for_existing_file() {
        let mut fs = test_helpers::MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\n---\nbody");
        let result = read_skill_preamble(Path::new(".ai/p/skills/s/SKILL.md"), &fs);
        assert!(result.is_some());
        let (source_type, skill) = result.unwrap();
        assert_eq!(source_type, ".ai");
        assert!(skill.frontmatter.is_some());
    }

    #[test]
    fn read_skill_preamble_returns_none_for_missing_file() {
        let fs = test_helpers::MockFs::new();
        assert!(read_skill_preamble(Path::new(".ai/p/skills/s/SKILL.md"), &fs).is_none());
    }

    #[test]
    fn read_agent_preamble_returns_some_for_existing_file() {
        let mut fs = test_helpers::MockFs::new();
        fs.add_agent("p", "reviewer", "---\nname: reviewer\ntools: Read\n---\nprompt");
        let result = read_agent_preamble(Path::new(".ai/p/agents/reviewer.md"), &fs);
        assert!(result.is_some());
        let (source_type, agent) = result.unwrap();
        assert_eq!(source_type, ".ai");
        assert!(agent.frontmatter.is_some());
    }

    #[test]
    fn read_agent_preamble_returns_none_for_missing_file() {
        let fs = test_helpers::MockFs::new();
        assert!(read_agent_preamble(Path::new(".ai/p/agents/reviewer.md"), &fs).is_none());
    }

    #[test]
    fn read_hook_preamble_returns_some_for_existing_file() {
        let mut fs = test_helpers::MockFs::new();
        fs.add_hooks("p", r#"{"preToolUse": []}"#);
        let result = read_hook_preamble(Path::new(".ai/p/hooks/hooks.json"), &fs);
        assert!(result.is_some());
        let (source_type, content) = result.unwrap();
        assert_eq!(source_type, ".ai");
        assert!(content.contains("preToolUse"));
    }

    #[test]
    fn read_hook_preamble_returns_none_for_missing_file() {
        let fs = test_helpers::MockFs::new();
        assert!(read_hook_preamble(Path::new(".ai/p/hooks/hooks.json"), &fs).is_none());
    }

    #[test]
    fn quality_rules_for_skill_kind() {
        let config = Config::default();
        let rules = quality_rules_for_kind(&FeatureKind::Skill, &config);
        assert!(!rules.is_empty());
        assert!(rules.iter().any(|r| r.id() == "skill/missing-name"));
    }

    #[test]
    fn quality_rules_for_agent_kind() {
        let config = Config::default();
        let rules = quality_rules_for_kind(&FeatureKind::Agent, &config);
        assert!(!rules.is_empty());
        assert!(rules.iter().any(|r| r.id() == "agent/missing-tools"));
    }

    #[test]
    fn quality_rules_for_hook_kind() {
        let config = Config::default();
        let rules = quality_rules_for_kind(&FeatureKind::Hook, &config);
        assert!(!rules.is_empty());
        assert!(rules.iter().any(|r| r.id() == "hook/unknown-event"));
    }

    #[test]
    fn quality_rules_for_plugin_kind_includes_broken_paths() {
        let config = Config::default();
        let rules = quality_rules_for_kind(&FeatureKind::Plugin, &config);
        assert!(!rules.is_empty());
        assert!(rules.iter().any(|r| r.id() == "plugin/broken-paths"));
    }

    #[test]
    fn quality_rules_for_marketplace_kind() {
        let config = Config::default();
        let rules = quality_rules_for_kind(&FeatureKind::Marketplace, &config);
        assert!(!rules.is_empty());
        assert!(rules.iter().any(|r| r.id() == "marketplace/source-resolve"));
        assert!(rules.iter().any(|r| r.id() == "marketplace/plugin-field-mismatch"));
        assert!(rules.iter().any(|r| r.id() == "plugin/missing-registration"));
        assert!(rules.iter().any(|r| r.id() == "plugin/missing-manifest"));
    }

    #[test]
    fn quality_rules_for_plugin_json_kind() {
        let config = Config::default();
        let rules = quality_rules_for_kind(&FeatureKind::PluginJson, &config);
        assert!(!rules.is_empty());
        assert!(rules.iter().any(|r| r.id() == "plugin/required-fields"));
    }

    #[test]
    fn quality_rules_for_instructions_kind() {
        let config = Config::default();
        let rules = quality_rules_for_kind(&FeatureKind::Instructions, &config);
        assert!(rules.iter().any(|r| r.id() == "instructions/oversized"));
    }
}
