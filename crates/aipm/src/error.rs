//! CLI-level error type for the `aipm` binary.
//!
//! Unifies every library error into a single enum so that `run()` and all
//! `cmd_*` helpers can use the `?` operator directly instead of manually
//! converting each error into `Box<dyn Error>`.

/// Unified error type for the aipm CLI.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// I/O errors (filesystem, env vars, etc.).
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Workspace initialisation errors.
    #[error(transparent)]
    WorkspaceInit(#[from] libaipm::workspace_init::Error),

    /// Package installer errors.
    #[error(transparent)]
    Installer(#[from] libaipm::installer::error::Error),

    /// Lint engine errors.
    #[error(transparent)]
    Lint(#[from] libaipm::lint::Error),

    /// Migration errors.
    #[error(transparent)]
    Migrate(#[from] libaipm::migrate::Error),

    /// Logging initialisation errors.
    #[error(transparent)]
    Logging(#[from] libaipm::logging::Error),

    /// Global installed-registry errors.
    #[error(transparent)]
    Installed(#[from] libaipm::installed::Error),

    /// Locked-file (advisory lock) errors.
    #[error(transparent)]
    LockedFile(#[from] libaipm::locked_file::Error),

    /// Linker errors (symlinks, link state, gitignore).
    #[error(transparent)]
    Linker(#[from] libaipm::linker::error::Error),

    /// Make (scaffolding) errors.
    #[error(transparent)]
    Make(#[from] libaipm::make::Error),

    /// Package init errors.
    #[error(transparent)]
    Init(#[from] libaipm::init::Error),

    /// Lockfile parse / version errors.
    #[error(transparent)]
    Lockfile(#[from] libaipm::lockfile::error::Error),

    /// Manifest parse / validation errors.
    #[error(transparent)]
    Manifest(#[from] libaipm::manifest::error::Error),

    /// JSON serialisation errors.
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    /// Ad-hoc message errors (replaces bare string `.into()` conversions).
    #[error("{0}")]
    Message(String),
}

// Manual `From` impls for types that cannot use `#[from]`.

impl From<String> for CliError {
    fn from(msg: String) -> Self {
        Self::Message(msg)
    }
}

impl From<&str> for CliError {
    fn from(msg: &str) -> Self {
        Self::Message(msg.to_string())
    }
}

impl From<Box<dyn std::error::Error>> for CliError {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        Self::Message(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        let cli: CliError = io_err.into();
        assert!(matches!(cli, CliError::Io(_)));
        assert!(cli.to_string().contains("gone"));
    }

    #[test]
    fn from_string() {
        let cli: CliError = String::from("bad thing").into();
        assert!(matches!(cli, CliError::Message(_)));
        assert_eq!(cli.to_string(), "bad thing");
    }

    #[test]
    fn from_str() {
        let cli: CliError = "oops".into();
        assert!(matches!(cli, CliError::Message(_)));
        assert_eq!(cli.to_string(), "oops");
    }

    #[test]
    fn from_boxed_error() {
        let boxed: Box<dyn std::error::Error> = "boxed error".into();
        let cli: CliError = boxed.into();
        assert!(matches!(cli, CliError::Message(_)));
        assert_eq!(cli.to_string(), "boxed error");
    }

    #[test]
    fn from_serde_json_error() {
        // Intentionally invalid JSON — parse must fail.
        let json_err = match serde_json::from_str::<serde_json::Value>("{{invalid") {
            Err(e) => e,
            Ok(_) => return,
        };
        let cli: CliError = json_err.into();
        assert!(matches!(cli, CliError::Json(_)));
    }
}
