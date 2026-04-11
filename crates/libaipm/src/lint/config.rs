//! Lint configuration from `[workspace.lints]` in `aipm.toml`.

use std::collections::BTreeMap;

use super::diagnostic::Severity;

/// Parsed lint configuration.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Global ignore paths (from `[workspace.lints.ignore].paths`).
    pub ignore_paths: Vec<String>,
    /// Per-rule overrides keyed by rule ID.
    pub rule_overrides: BTreeMap<String, RuleOverride>,
}

/// Override for a single lint rule.
#[derive(Debug, Clone)]
pub enum RuleOverride {
    /// Suppress the rule entirely.
    Allow,
    /// Simple severity override.
    Level(Severity),
    /// Detailed override with severity, per-rule ignore paths, and custom options.
    Detailed {
        /// Severity level.
        level: Severity,
        /// Per-rule ignore paths (globs).
        ignore: Vec<String>,
        /// Per-rule custom options forwarded from the TOML config.
        options: BTreeMap<String, toml::Value>,
    },
}

impl Config {
    /// Check if a rule is suppressed (set to `"allow"` in config).
    pub fn is_suppressed(&self, rule_id: &str) -> bool {
        matches!(self.rule_overrides.get(rule_id), Some(RuleOverride::Allow))
    }

    /// Get the severity override for a rule, if any.
    pub fn severity_override(&self, rule_id: &str) -> Option<Severity> {
        match self.rule_overrides.get(rule_id) {
            Some(RuleOverride::Level(s)) => Some(*s),
            Some(RuleOverride::Detailed { level, .. }) => Some(*level),
            _ => None,
        }
    }

    /// Get per-rule ignore paths, if any.
    pub fn rule_ignore_paths(&self, rule_id: &str) -> &[String] {
        match self.rule_overrides.get(rule_id) {
            Some(RuleOverride::Detailed { ignore, .. }) => ignore,
            _ => &[],
        }
    }

    /// Get per-rule custom options, if any.
    ///
    /// Returns the options `BTreeMap` for `Detailed` overrides, or an empty map otherwise.
    pub fn rule_options<'a>(&'a self, rule_id: &str) -> &'a BTreeMap<String, toml::Value> {
        static EMPTY: std::sync::OnceLock<BTreeMap<String, toml::Value>> =
            std::sync::OnceLock::new();
        if let Some(RuleOverride::Detailed { options, .. }) = self.rule_overrides.get(rule_id) {
            options
        } else {
            EMPTY.get_or_init(BTreeMap::new)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_no_overrides() {
        let config = Config::default();
        assert!(config.ignore_paths.is_empty());
        assert!(config.rule_overrides.is_empty());
    }

    #[test]
    fn is_suppressed_returns_true_for_allow() {
        let mut config = Config::default();
        config.rule_overrides.insert("skill/oversized".to_string(), RuleOverride::Allow);
        assert!(config.is_suppressed("skill/oversized"));
        assert!(!config.is_suppressed("skill/missing-name"));
    }

    #[test]
    fn severity_override_returns_level() {
        let mut config = Config::default();
        config
            .rule_overrides
            .insert("skill/missing-description".to_string(), RuleOverride::Level(Severity::Error));
        assert_eq!(config.severity_override("skill/missing-description"), Some(Severity::Error));
        assert_eq!(config.severity_override("nonexistent"), None);
    }

    #[test]
    fn severity_override_from_detailed() {
        let mut config = Config::default();
        config.rule_overrides.insert(
            "plugin/broken-paths".to_string(),
            RuleOverride::Detailed {
                level: Severity::Warning,
                ignore: vec!["examples/**".to_string()],
                options: BTreeMap::new(),
            },
        );
        assert_eq!(config.severity_override("plugin/broken-paths"), Some(Severity::Warning));
    }

    #[test]
    fn rule_ignore_paths_returns_empty_for_simple() {
        let mut config = Config::default();
        config
            .rule_overrides
            .insert("skill/oversized".to_string(), RuleOverride::Level(Severity::Error));
        assert!(config.rule_ignore_paths("skill/oversized").is_empty());
    }

    #[test]
    fn rule_ignore_paths_returns_paths_for_detailed() {
        let mut config = Config::default();
        config.rule_overrides.insert(
            "plugin/broken-paths".to_string(),
            RuleOverride::Detailed {
                level: Severity::Error,
                ignore: vec!["examples/**".to_string(), "vendor/**".to_string()],
                options: BTreeMap::new(),
            },
        );
        let paths = config.rule_ignore_paths("plugin/broken-paths");
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn is_suppressed_returns_false_for_level() {
        let mut config = Config::default();
        config
            .rule_overrides
            .insert("skill/oversized".to_string(), RuleOverride::Level(Severity::Warning));
        assert!(!config.is_suppressed("skill/oversized"));
    }

    #[test]
    fn severity_override_returns_none_for_allow() {
        let mut config = Config::default();
        config.rule_overrides.insert("skill/oversized".to_string(), RuleOverride::Allow);
        assert_eq!(config.severity_override("skill/oversized"), None);
    }

    #[test]
    fn rule_override_detailed_with_options_stores_and_returns() {
        let mut config = Config::default();
        let mut opts = BTreeMap::new();
        opts.insert("lines".to_string(), toml::Value::Integer(200));
        config.rule_overrides.insert(
            "instructions/oversized".to_string(),
            RuleOverride::Detailed { level: Severity::Error, ignore: vec![], options: opts },
        );
        let returned = config.rule_options("instructions/oversized");
        assert_eq!(returned.get("lines"), Some(&toml::Value::Integer(200)));
    }

    #[test]
    fn rule_options_empty_for_allow_variant() {
        let mut config = Config::default();
        config.rule_overrides.insert("skill/oversized".to_string(), RuleOverride::Allow);
        assert!(config.rule_options("skill/oversized").is_empty());
    }

    #[test]
    fn rule_options_empty_for_level_variant() {
        let mut config = Config::default();
        config
            .rule_overrides
            .insert("skill/oversized".to_string(), RuleOverride::Level(Severity::Warning));
        assert!(config.rule_options("skill/oversized").is_empty());
    }

    #[test]
    fn rule_options_empty_for_missing_rule() {
        let config = Config::default();
        assert!(config.rule_options("nonexistent/rule").is_empty());
    }
}
