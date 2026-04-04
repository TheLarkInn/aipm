//! Lint rule implementations and factory functions.
//!
//! The primary entry points are [`quality_rules_for_kind()`] (kind-based dispatch
//! for the unified discovery pipeline) and [`misplaced_features_rule()`] (produces
//! a per-feature `source/misplaced-features` rule instance).

pub mod agent_missing_tools;
pub mod broken_paths;
pub mod hook_legacy_event;
pub mod hook_unknown_event;
pub mod known_events;
pub mod misplaced_features;
mod scan;
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

use super::rule::Rule;

/// Get quality rules applicable to a feature kind.
///
/// These rules validate individual feature files without regard to which
/// source directory the feature came from.
pub(crate) fn quality_rules_for_kind(kind: &FeatureKind) -> Vec<Box<dyn Rule>> {
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
        FeatureKind::Plugin => vec![],
    }
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

    #[test]
    fn quality_rules_for_skill_kind() {
        let rules = quality_rules_for_kind(&FeatureKind::Skill);
        assert!(!rules.is_empty());
        assert!(rules.iter().any(|r| r.id() == "skill/missing-name"));
    }

    #[test]
    fn quality_rules_for_agent_kind() {
        let rules = quality_rules_for_kind(&FeatureKind::Agent);
        assert!(!rules.is_empty());
        assert!(rules.iter().any(|r| r.id() == "agent/missing-tools"));
    }

    #[test]
    fn quality_rules_for_hook_kind() {
        let rules = quality_rules_for_kind(&FeatureKind::Hook);
        assert!(!rules.is_empty());
        assert!(rules.iter().any(|r| r.id() == "hook/unknown-event"));
    }

    #[test]
    fn quality_rules_for_plugin_kind_is_empty() {
        let rules = quality_rules_for_kind(&FeatureKind::Plugin);
        assert!(rules.is_empty());
    }
}
