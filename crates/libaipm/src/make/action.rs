use std::path::PathBuf;

/// An atomic, idempotent scaffolding action.
///
/// Each variant records a single operation performed (or skipped) by
/// `make_plugin()`.  The CLI layer iterates the returned `Vec<Action>`
/// and renders human-readable output.
///
/// Designed for shared use between `aipm make` and future `lint --fix`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Created a new directory.
    DirectoryCreated {
        /// Absolute path to the directory.
        path: PathBuf,
    },
    /// Directory already existed (idempotent skip).
    DirectoryAlreadyExists {
        /// Absolute path to the directory.
        path: PathBuf,
    },

    /// Wrote a new file.
    FileWritten {
        /// Absolute path to the file.
        path: PathBuf,
        /// Human-readable description of the file purpose.
        description: String,
    },
    /// File already existed (idempotent skip).
    FileAlreadyExists {
        /// Absolute path to the file.
        path: PathBuf,
    },

    /// Registered plugin in marketplace.json.
    PluginRegistered {
        /// Plugin name.
        name: String,
        /// Path to the marketplace.json file.
        marketplace_path: PathBuf,
    },
    /// Plugin was already registered (idempotent skip).
    PluginAlreadyRegistered {
        /// Plugin name.
        name: String,
    },

    /// Enabled plugin in engine settings.
    PluginEnabled {
        /// Plugin key (e.g., "my-plugin@marketplace-name").
        plugin_key: String,
        /// Path to the engine settings file.
        settings_path: PathBuf,
    },
    /// Plugin was already enabled (idempotent skip).
    PluginAlreadyEnabled {
        /// Plugin key.
        plugin_key: String,
    },

    /// Top-level summary: a complete plugin was created.
    PluginCreated {
        /// Plugin name.
        name: String,
        /// Path to the plugin directory.
        path: PathBuf,
        /// List of feature CLI names that were scaffolded.
        features: Vec<String>,
        /// Target engine (e.g., "claude", "copilot", "both").
        engine: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_debug_and_clone() {
        let action = Action::DirectoryCreated { path: PathBuf::from("/tmp/test") };
        let cloned = action.clone();
        assert_eq!(action, cloned);
        let debug = format!("{action:?}");
        assert!(debug.contains("DirectoryCreated"));
    }

    #[test]
    fn action_variants_equality() {
        let a = Action::FileWritten { path: PathBuf::from("/a"), description: "desc".to_string() };
        let b = Action::FileWritten { path: PathBuf::from("/a"), description: "desc".to_string() };
        assert_eq!(a, b);

        let c = Action::FileWritten { path: PathBuf::from("/b"), description: "other".to_string() };
        assert_ne!(a, c);
    }

    #[test]
    fn plugin_created_contains_features() {
        let action = Action::PluginCreated {
            name: "my-plugin".to_string(),
            path: PathBuf::from(".ai/my-plugin"),
            features: vec!["skill".to_string(), "agent".to_string()],
            engine: "claude".to_string(),
        };
        if let Action::PluginCreated { features, .. } = &action {
            assert_eq!(features.len(), 2);
            assert_eq!(features[0], "skill");
            assert_eq!(features[1], "agent");
        }
    }

    #[test]
    fn idempotent_variants_exist() {
        let already_dir = Action::DirectoryAlreadyExists { path: PathBuf::from("/x") };
        let already_file = Action::FileAlreadyExists { path: PathBuf::from("/y") };
        let already_reg = Action::PluginAlreadyRegistered { name: "p".to_string() };
        let already_enabled = Action::PluginAlreadyEnabled { plugin_key: "k".to_string() };
        // Verify they are distinct variants via debug output
        assert!(format!("{already_dir:?}").contains("DirectoryAlreadyExists"));
        assert!(format!("{already_file:?}").contains("FileAlreadyExists"));
        assert!(format!("{already_reg:?}").contains("PluginAlreadyRegistered"));
        assert!(format!("{already_enabled:?}").contains("PluginAlreadyEnabled"));
    }
}
