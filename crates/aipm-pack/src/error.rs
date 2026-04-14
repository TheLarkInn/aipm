//! CLI-level error type for the `aipm-pack` binary.
//!
//! Unifies every library error into a single enum so that `run()` can
//! use the `?` operator directly instead of `Box<dyn Error>`.

/// Unified error type for the aipm-pack CLI.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// I/O errors (filesystem, env vars, etc.).
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Package init errors.
    #[error(transparent)]
    Init(#[from] libaipm::init::Error),

    /// Ad-hoc message errors.
    #[error("{0}")]
    Message(String),
}

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
}
