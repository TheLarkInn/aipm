//! Error types for the `aipm make` command.

/// Errors that can occur during `aipm make` operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No `.ai/` marketplace found in this or any parent directory.
    #[error("marketplace not found (run `aipm init` first)")]
    MarketplaceNotFound,

    /// The plugin name failed validation.
    #[error("invalid plugin name: {0}")]
    InvalidName(String),

    /// A requested feature is not supported by the target engine.
    #[error("feature {feature} is not supported by engine {engine}")]
    UnsupportedFeature {
        /// The unsupported feature CLI name.
        feature: String,
        /// The target engine name.
        engine: String,
    },

    /// An unrecognised engine string was provided.
    #[error("invalid engine: {0} (expected: claude, copilot, both)")]
    InvalidEngine(String),

    /// An unrecognised feature string was provided.
    #[error(
        "invalid feature: {0} (expected: skill, agent, mcp, hook, output-style, lsp, extension)"
    )]
    InvalidFeature(String),

    /// A required CLI flag was not provided in non-interactive mode.
    #[error("missing required flag: --{0}")]
    MissingFlag(String),

    /// I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// JSON serialization / deserialization error.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marketplace_not_found_message() {
        let err = Error::MarketplaceNotFound;
        let msg = err.to_string();
        assert!(msg.contains("marketplace not found"));
        assert!(msg.contains("aipm init"));
    }

    #[test]
    fn invalid_name_message() {
        let err = Error::InvalidName("BAD NAME".to_string());
        assert_eq!(err.to_string(), "invalid plugin name: BAD NAME");
    }

    #[test]
    fn unsupported_feature_message() {
        let err =
            Error::UnsupportedFeature { feature: "lsp".to_string(), engine: "claude".to_string() };
        assert_eq!(err.to_string(), "feature lsp is not supported by engine claude");
    }

    #[test]
    fn invalid_engine_message() {
        let err = Error::InvalidEngine("foobar".to_string());
        let msg = err.to_string();
        assert!(msg.contains("foobar"));
        assert!(msg.contains("expected: claude, copilot, both"));
    }

    #[test]
    fn invalid_feature_message() {
        let err = Error::InvalidFeature("widget".to_string());
        let msg = err.to_string();
        assert!(msg.contains("widget"));
        assert!(msg.contains("expected: skill, agent, mcp, hook"));
    }

    #[test]
    fn missing_flag_message() {
        let err = Error::MissingFlag("name".to_string());
        assert_eq!(err.to_string(), "missing required flag: --name");
    }

    #[test]
    fn io_error_transparent() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        let err = Error::from(io_err);
        assert!(err.to_string().contains("gone"));
    }

    #[test]
    fn json_error_transparent() {
        // Trigger a real serde_json error by parsing invalid JSON
        let json_err = serde_json::from_str::<serde_json::Value>("{{bad").unwrap_err();
        let err = Error::from(json_err);
        assert!(!err.to_string().is_empty());
    }
}
