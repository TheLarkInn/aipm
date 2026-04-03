//! Logging initialization for the aipm CLI.
//!
//! Provides a layered `tracing` subscriber with:
//! - **stderr layer**: filtered by CLI verbosity flags or `AIPM_LOG` env var
//! - **file layer**: always-on at DEBUG level, daily rotation, 7-day retention

use std::io;

use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter, Layer, Registry};

/// Log output format for the stderr layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// Human-readable text output.
    Text,
    /// Machine-readable JSON output (for agentic consumers).
    Json,
}

/// Error returned when logging initialization fails.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Failed to create the rolling file appender.
    #[error("failed to initialize file appender: {reason}")]
    FileAppender { reason: String },

    /// Failed to set the global tracing subscriber.
    #[error("failed to set global tracing subscriber: {reason}")]
    SetGlobal { reason: String },
}

/// Build the `EnvFilter` for the stderr layer.
///
/// If `AIPM_LOG` is set, it takes precedence over CLI verbosity flags.
fn stderr_filter(verbosity: LevelFilter) -> EnvFilter {
    if std::env::var_os("AIPM_LOG").is_some() {
        EnvFilter::from_env("AIPM_LOG")
    } else {
        // Convert LevelFilter to a string that EnvFilter understands
        let directive = match verbosity {
            LevelFilter::OFF => "off",
            LevelFilter::ERROR => "error",
            LevelFilter::WARN => "warn",
            LevelFilter::INFO => "info",
            LevelFilter::DEBUG => "debug",
            LevelFilter::TRACE => "trace",
        };
        EnvFilter::new(directive)
    }
}

/// Initialize the global tracing subscriber.
///
/// Creates two layers:
/// 1. **stderr** — filtered by `AIPM_LOG` env var (if set) or the CLI `verbosity` level
/// 2. **file** — always at `DEBUG`, written to `<temp_dir>/aipm-YYYY-MM-DD.log`
///    with daily rotation and 7-day retention
///
/// # Errors
///
/// Returns an error if the file appender cannot be created or the global
/// subscriber cannot be set (e.g., if called twice).
pub fn init(verbosity: LevelFilter, format: LogFormat) -> Result<(), Error> {
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("aipm")
        .filename_suffix("log")
        .max_log_files(7)
        .build(std::env::temp_dir())
        .map_err(|e| Error::FileAppender { reason: e.to_string() })?;

    let file_layer = fmt::layer()
        .with_writer(file_appender)
        .with_ansi(false)
        .with_target(true)
        .with_filter(LevelFilter::DEBUG);

    let env_filter = stderr_filter(verbosity);

    let stderr_layer: Box<dyn Layer<Registry> + Send + Sync> = match format {
        LogFormat::Text => {
            Box::new(fmt::layer().with_writer(io::stderr).with_target(true).with_filter(env_filter))
        },
        LogFormat::Json => Box::new(
            fmt::layer()
                .json()
                .with_writer(io::stderr)
                .with_target(true)
                .with_file(true)
                .with_line_number(true)
                .with_filter(env_filter),
        ),
    };

    tracing_subscriber::registry()
        .with(stderr_layer)
        .with(file_layer)
        .try_init()
        .map_err(|e| Error::SetGlobal { reason: e.to_string() })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_format_equality() {
        assert_eq!(LogFormat::Text, LogFormat::Text);
        assert_eq!(LogFormat::Json, LogFormat::Json);
        assert_ne!(LogFormat::Text, LogFormat::Json);
    }

    #[test]
    fn log_format_debug() {
        let text = format!("{:?}", LogFormat::Text);
        assert!(text.contains("Text"));
        let json = format!("{:?}", LogFormat::Json);
        assert!(json.contains("Json"));
    }

    #[test]
    fn stderr_filter_uses_verbosity_when_no_env() {
        // Ensure AIPM_LOG is not set for this test
        std::env::remove_var("AIPM_LOG");
        let filter = stderr_filter(LevelFilter::WARN);
        let debug_repr = format!("{filter:?}");
        let lower = debug_repr.to_lowercase();
        assert!(lower.contains("warn"), "filter should contain warn directive: {debug_repr}");
    }

    #[test]
    fn stderr_filter_uses_env_when_set() {
        std::env::set_var("AIPM_LOG", "debug");
        let filter = stderr_filter(LevelFilter::WARN);
        let debug_repr = format!("{filter:?}");
        let lower = debug_repr.to_lowercase();
        assert!(lower.contains("debug"), "filter should use AIPM_LOG value: {debug_repr}");
        std::env::remove_var("AIPM_LOG");
    }

    #[test]
    fn error_display() {
        let e = Error::FileAppender { reason: "test".to_string() };
        assert!(e.to_string().contains("file appender"));

        let e = Error::SetGlobal { reason: "test".to_string() };
        assert!(e.to_string().contains("global tracing subscriber"));
    }
}
