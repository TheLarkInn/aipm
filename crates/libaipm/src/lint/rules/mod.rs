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

use crate::discovery::{DiscoveredFeature, FeatureKind};
use misplaced_features::MisplacedFeatures;

use super::config::Config;
use super::rule::Rule;

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
