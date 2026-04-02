//! Lint rule implementations and factory functions.
//!
//! Each source type has a factory function that returns its rule set,
//! following the same adapter pattern as migrate detectors.

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

use std::path::Path;

use super::rule::Rule;

/// Rules for validating `.claude/` source directories.
pub(crate) fn for_claude(project_root: &Path) -> Vec<Box<dyn Rule>> {
    vec![Box::new(misplaced_features::MisplacedFeatures {
        source_type: ".claude",
        project_root: project_root.to_path_buf(),
    })]
}

/// Rules for validating `.github/` source directories.
pub(crate) fn for_copilot(project_root: &Path) -> Vec<Box<dyn Rule>> {
    vec![Box::new(misplaced_features::MisplacedFeatures {
        source_type: ".github",
        project_root: project_root.to_path_buf(),
    })]
}

/// Rules for validating `.ai/` marketplace plugins.
pub(crate) fn for_marketplace() -> Vec<Box<dyn Rule>> {
    vec![
        // Core rules (from BDD spec + issue #110)
        Box::new(skill_missing_name::MissingName),
        Box::new(skill_missing_desc::MissingDescription),
        Box::new(skill_oversized::Oversized),
        Box::new(agent_missing_tools::MissingTools),
        Box::new(hook_unknown_event::UnknownEvent),
        Box::new(broken_paths::BrokenPaths),
        // Cross-tool compatibility rules (from binary analysis)
        Box::new(skill_name_too_long::NameTooLong),
        Box::new(skill_name_invalid::NameInvalidChars),
        Box::new(skill_desc_too_long::DescriptionTooLong),
        Box::new(skill_invalid_shell::InvalidShell),
        Box::new(hook_legacy_event::LegacyEventName),
    ]
}

/// Dispatch: source type string -> rule set.
///
/// For `.claude` and `.github`, `project_root` is used to locate the `.ai/`
/// marketplace directory. For `.ai` and unknown sources, it is ignored.
pub(crate) fn for_source(source: &str, project_root: &Path) -> Vec<Box<dyn Rule>> {
    match source {
        ".claude" => for_claude(project_root),
        ".github" => for_copilot(project_root),
        ".ai" => for_marketplace(),
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_source_returns_empty() {
        assert!(for_source(".unknown", std::path::Path::new(".")).is_empty());
    }

    #[test]
    fn claude_returns_rules() {
        let rules = for_source(".claude", std::path::Path::new("."));
        let _ = rules;
    }
}
