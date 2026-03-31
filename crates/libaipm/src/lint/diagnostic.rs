//! Diagnostic types for lint findings.

use std::fmt;
use std::path::PathBuf;

/// Severity level for a lint diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Advisory finding — does not cause a non-zero exit code.
    Warning,
    /// Blocking finding — causes exit code 1.
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Warning => f.write_str("warning"),
            Self::Error => f.write_str("error"),
        }
    }
}

impl Severity {
    /// Parse a severity from a config string (`"error"`, `"warn"`, `"warning"`).
    ///
    /// Returns `None` for unrecognized values (including `"allow"`, which is
    /// handled separately by `LintConfig`).
    pub fn from_str_config(s: &str) -> Option<Self> {
        match s {
            "error" | "deny" => Some(Self::Error),
            "warn" | "warning" => Some(Self::Warning),
            _ => None,
        }
    }
}

/// A single lint finding.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// Hierarchical rule ID (e.g., `"skill/missing-description"`).
    pub rule_id: String,
    /// Severity level.
    pub severity: Severity,
    /// Human-readable message.
    pub message: String,
    /// File path where the issue was found (relative to workspace root).
    pub file_path: PathBuf,
    /// Optional 1-based line number.
    pub line: Option<usize>,
    /// Source type that produced this diagnostic (e.g., `".claude"`, `".ai"`).
    pub source_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_ordering_warning_less_than_error() {
        assert!(Severity::Warning < Severity::Error);
    }

    #[test]
    fn severity_display() {
        assert_eq!(format!("{}", Severity::Warning), "warning");
        assert_eq!(format!("{}", Severity::Error), "error");
    }

    #[test]
    fn severity_from_str_config_valid() {
        assert_eq!(Severity::from_str_config("error"), Some(Severity::Error));
        assert_eq!(Severity::from_str_config("deny"), Some(Severity::Error));
        assert_eq!(Severity::from_str_config("warn"), Some(Severity::Warning));
        assert_eq!(Severity::from_str_config("warning"), Some(Severity::Warning));
    }

    #[test]
    fn severity_from_str_config_invalid() {
        assert_eq!(Severity::from_str_config("allow"), None);
        assert_eq!(Severity::from_str_config("info"), None);
        assert_eq!(Severity::from_str_config(""), None);
    }

    #[test]
    fn diagnostic_construction() {
        let d = Diagnostic {
            rule_id: "skill/missing-description".to_string(),
            severity: Severity::Warning,
            message: "SKILL.md missing required field: description".to_string(),
            file_path: PathBuf::from(".ai/my-plugin/skills/default/SKILL.md"),
            line: Some(1),
            source_type: ".ai".to_string(),
        };
        assert_eq!(d.rule_id, "skill/missing-description");
        assert_eq!(d.severity, Severity::Warning);
        assert!(d.line.is_some());
    }
}
