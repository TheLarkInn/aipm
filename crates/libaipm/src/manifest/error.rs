//! Structured error types for manifest parsing and validation.
//!
//! Errors include context about what went wrong and where, designed for
//! AI-friendly diagnostics.

use std::fmt;
use std::path::PathBuf;

/// All possible errors from manifest parsing and validation.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// TOML syntax or structure error.
    #[error("failed to parse manifest: {source}")]
    Parse {
        /// The underlying TOML deserialization error.
        source: toml::de::Error,
    },

    /// A required field is absent.
    #[error("missing required field: {field}")]
    MissingField {
        /// Name of the missing field.
        field: String,
    },

    /// The package name doesn't match naming rules.
    #[error("invalid package name: {name} — {reason}")]
    InvalidName {
        /// The invalid name value.
        name: String,
        /// Why it's invalid.
        reason: String,
    },

    /// The version string is not valid semver.
    #[error("invalid semver version: {version}")]
    InvalidVersion {
        /// The invalid version string.
        version: String,
    },

    /// A dependency version requirement is unparseable.
    #[error("invalid version requirement for dependency: {dependency}")]
    InvalidDependencyVersion {
        /// The dependency with the bad version.
        dependency: String,
        /// The invalid version string.
        version: String,
    },

    /// An unknown plugin type was specified.
    #[error("invalid plugin type: {value} — expected one of: skill, agent, mcp, hook, composite")]
    InvalidPluginType {
        /// The invalid type value.
        value: String,
    },

    /// A declared component path does not exist on disk.
    #[error("component not found: {}", path.display())]
    ComponentNotFound {
        /// The missing component path.
        path: PathBuf,
    },

    /// I/O error reading the manifest file.
    #[error("failed to read manifest: {source}")]
    Io {
        /// The underlying I/O error.
        source: std::io::Error,
    },

    /// Multiple validation errors collected.
    #[error("{}", format_errors(.0))]
    Multiple(Vec<Self>),
}

fn format_errors(errors: &[Error]) -> String {
    let mut buf = String::new();
    for (i, e) in errors.iter().enumerate() {
        if i > 0 {
            buf.push_str("; ");
        }
        buf.push_str(&fmt::format(format_args!("{e}")));
    }
    buf
}
