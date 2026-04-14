//! Engine-to-feature mapping for `aipm make plugin`.
//!
//! Codifies which AI feature types are available for each engine
//! (Claude, Copilot, or both) and provides validation.

/// A feature that can be included in a plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Feature {
    /// Skills — prompt templates.
    Skill,
    /// Agents — autonomous sub-agents.
    Agent,
    /// MCP Servers — tool providers.
    Mcp,
    /// Hooks — lifecycle events.
    Hook,
    /// Output Styles — response formatting (Claude only).
    OutputStyle,
    /// LSP Servers — language intelligence (Copilot only).
    Lsp,
    /// Extensions — Copilot extensions (Copilot only).
    Extension,
}

impl Feature {
    /// CLI flag value (used in `--feature`).
    #[must_use]
    pub const fn cli_name(&self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::Agent => "agent",
            Self::Mcp => "mcp",
            Self::Hook => "hook",
            Self::OutputStyle => "output-style",
            Self::Lsp => "lsp",
            Self::Extension => "extension",
        }
    }

    /// Human-readable label for wizard prompts.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Skill => "Skills (prompt templates)",
            Self::Agent => "Agents (autonomous sub-agents)",
            Self::Mcp => "MCP Servers (tool providers)",
            Self::Hook => "Hooks (lifecycle events)",
            Self::OutputStyle => "Output Styles (response formatting)",
            Self::Lsp => "LSP Servers (language intelligence)",
            Self::Extension => "Extensions (Copilot extensions)",
        }
    }

    /// Parse from a CLI flag value.
    #[must_use]
    pub fn from_cli_name(s: &str) -> Option<Self> {
        match s {
            "skill" => Some(Self::Skill),
            "agent" => Some(Self::Agent),
            "mcp" => Some(Self::Mcp),
            "hook" => Some(Self::Hook),
            "output-style" => Some(Self::OutputStyle),
            "lsp" => Some(Self::Lsp),
            "extension" => Some(Self::Extension),
            _ => None,
        }
    }

    /// All known features.
    const ALL: [Self; 7] = [
        Self::Skill,
        Self::Agent,
        Self::Mcp,
        Self::Hook,
        Self::OutputStyle,
        Self::Lsp,
        Self::Extension,
    ];
}

/// Features supported by Claude Code.
const CLAUDE_FEATURES: [Feature; 5] =
    [Feature::Skill, Feature::Agent, Feature::Mcp, Feature::Hook, Feature::OutputStyle];

/// Features supported by Copilot CLI.
const COPILOT_FEATURES: [Feature; 6] =
    [Feature::Skill, Feature::Agent, Feature::Mcp, Feature::Hook, Feature::Lsp, Feature::Extension];

/// Returns the features available for a given engine.
///
/// - `"claude"` — `Skill`, `Agent`, `Mcp`, `Hook`, `OutputStyle`
/// - `"copilot"` — `Skill`, `Agent`, `Mcp`, `Hook`, `Lsp`, `Extension`
/// - `"both"` — all 7 features
#[must_use]
pub fn features_for_engine(engine: &str) -> Vec<Feature> {
    match engine {
        "claude" => CLAUDE_FEATURES.to_vec(),
        "copilot" => COPILOT_FEATURES.to_vec(),
        "both" => Feature::ALL.to_vec(),
        _ => Vec::new(),
    }
}

/// Validates that all requested features are supported by the engine.
///
/// Returns `Ok(())` if every feature is supported, or `Err(unsupported)`
/// with the list of features that are not supported.
pub fn validate_features(engine: &str, features: &[Feature]) -> Result<(), Vec<Feature>> {
    let supported = features_for_engine(engine);
    let unsupported: Vec<Feature> =
        features.iter().copied().filter(|f| !supported.contains(f)).collect();
    if unsupported.is_empty() {
        Ok(())
    } else {
        Err(unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_features_claude() {
        let feats = features_for_engine("claude");
        assert_eq!(feats.len(), 5);
        assert!(feats.contains(&Feature::Skill));
        assert!(feats.contains(&Feature::Agent));
        assert!(feats.contains(&Feature::Mcp));
        assert!(feats.contains(&Feature::Hook));
        assert!(feats.contains(&Feature::OutputStyle));
        // Claude does NOT support LSP or Extension
        assert!(!feats.contains(&Feature::Lsp));
        assert!(!feats.contains(&Feature::Extension));
    }

    #[test]
    fn engine_features_copilot() {
        let feats = features_for_engine("copilot");
        assert_eq!(feats.len(), 6);
        assert!(feats.contains(&Feature::Skill));
        assert!(feats.contains(&Feature::Agent));
        assert!(feats.contains(&Feature::Mcp));
        assert!(feats.contains(&Feature::Hook));
        assert!(feats.contains(&Feature::Lsp));
        assert!(feats.contains(&Feature::Extension));
        // Copilot does NOT support OutputStyle
        assert!(!feats.contains(&Feature::OutputStyle));
    }

    #[test]
    fn engine_features_both() {
        let feats = features_for_engine("both");
        assert_eq!(feats.len(), 7);
        // Every variant should be present
        for f in &Feature::ALL {
            assert!(feats.contains(f), "missing feature: {:?}", f);
        }
    }

    #[test]
    fn engine_features_unknown_returns_empty() {
        let feats = features_for_engine("unknown");
        assert!(feats.is_empty());
    }

    #[test]
    fn validate_features_rejects_lsp_for_claude() {
        let result = validate_features("claude", &[Feature::Lsp]);
        assert!(result.is_err());
        let unsupported = result.unwrap_err();
        assert_eq!(unsupported, vec![Feature::Lsp]);
    }

    #[test]
    fn validate_features_rejects_output_style_for_copilot() {
        let result = validate_features("copilot", &[Feature::OutputStyle]);
        assert!(result.is_err());
        let unsupported = result.unwrap_err();
        assert_eq!(unsupported, vec![Feature::OutputStyle]);
    }

    #[test]
    fn validate_features_accepts_valid_combination() {
        assert!(validate_features("claude", &[Feature::Skill, Feature::Agent]).is_ok());
        assert!(validate_features("copilot", &[Feature::Skill, Feature::Lsp]).is_ok());
        assert!(validate_features("both", &Feature::ALL).is_ok());
    }

    #[test]
    fn validate_features_reports_multiple_unsupported() {
        let result = validate_features("claude", &[Feature::Lsp, Feature::Extension]);
        assert!(result.is_err());
        let unsupported = result.unwrap_err();
        assert_eq!(unsupported.len(), 2);
        assert!(unsupported.contains(&Feature::Lsp));
        assert!(unsupported.contains(&Feature::Extension));
    }

    #[test]
    fn from_cli_name_roundtrip() {
        for feature in &Feature::ALL {
            let name = feature.cli_name();
            let parsed = Feature::from_cli_name(name);
            assert_eq!(parsed, Some(*feature), "roundtrip failed for {name}");
        }
    }

    #[test]
    fn from_cli_name_unknown_returns_none() {
        assert_eq!(Feature::from_cli_name("widget"), None);
        assert_eq!(Feature::from_cli_name(""), None);
    }

    #[test]
    fn labels_are_non_empty() {
        for feature in &Feature::ALL {
            assert!(!feature.label().is_empty(), "empty label for {:?}", feature);
        }
    }
}
