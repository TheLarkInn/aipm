//! Rule: `source/misplaced-features` — plugin features in source dirs instead of marketplace.
//!
//! Checks `.claude/` or `.github/` for skills, agents, hooks, etc. that should be in `.ai/`.
//! Fires regardless of whether `.ai/` exists; help text varies based on `ai_exists`.

use std::path::Path;

use crate::discovery::DiscoveredFeature;
use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

use super::scan;

/// Checks for plugin features sitting in tool-specific directories.
pub(crate) struct MisplacedFeatures {
    /// Whether a `.ai/` marketplace directory exists in the project root.
    /// Controls the help text: when false, suggests `aipm init` first.
    pub ai_exists: bool,
}

impl Rule for MisplacedFeatures {
    fn id(&self) -> &'static str {
        "source/misplaced-features"
    }

    fn name(&self) -> &'static str {
        "misplaced plugin features"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

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

    fn check_file(&self, file_path: &Path, _fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let source_type = scan::source_type_from_path(file_path).to_string();
        Ok(vec![Diagnostic {
            rule_id: self.id().to_string(),
            severity: self.default_severity(),
            message: format!(
                "plugin feature found outside .ai/ marketplace: {}",
                file_path.display()
            ),
            file_path: file_path.to_path_buf(),
            line: None,
            col: None,
            end_line: None,
            end_col: None,
            source_type,
            help_text: None,
            help_url: None,
        }])
    }
}

/// Construct a `MisplacedFeatures` rule instance for a discovered feature.
///
/// The `_feature` parameter is accepted for call-site symmetry with the engine API;
/// only `ai_exists` affects behavior today.
pub(crate) const fn misplaced_features_rule(
    _feature: &DiscoveredFeature,
    ai_exists: bool,
) -> MisplacedFeatures {
    MisplacedFeatures { ai_exists }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::test_helpers::MockFs;

    #[test]
    fn help_text_with_marketplace() {
        let rule = MisplacedFeatures { ai_exists: true };
        assert!(rule.help_text().unwrap_or("").contains("aipm migrate"));
    }

    #[test]
    fn help_text_without_marketplace_suggests_init() {
        let rule = MisplacedFeatures { ai_exists: false };
        let text = rule.help_text().unwrap_or("");
        assert!(text.contains("aipm init"));
        assert!(text.contains("aipm migrate"));
    }

    #[test]
    fn check_file_produces_diagnostic() {
        let fs = MockFs::new();

        let rule = MisplacedFeatures { ai_exists: true };
        let result = rule.check_file(Path::new("/project/.github/skills/default/SKILL.md"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "source/misplaced-features");
        assert!(diags[0].message.contains("SKILL.md"));
        assert_eq!(diags[0].source_type, ".github");
    }

    #[test]
    fn check_file_without_marketplace() {
        let fs = MockFs::new();

        let rule = MisplacedFeatures { ai_exists: false };
        let result = rule.check_file(Path::new("/project/.claude/skills/s/SKILL.md"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].source_type, ".claude");
    }
}
