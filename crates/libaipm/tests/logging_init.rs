//! Integration test for `libaipm::logging::init`.
//!
//! Runs in its own process (a dedicated test binary) so that setting the
//! global `tracing` subscriber here cannot interfere with unit tests in the
//! `libaipm` lib crate, which run in a shared process.

use libaipm::logging::{init, LogFormat};
use tracing_subscriber::filter::LevelFilter;

/// A global tracing subscriber can only be set once per process. Calling
/// `init` a second time makes `try_init()` fail, exercising the
/// `Error::SetGlobal` branch.
#[test]
fn init_twice_returns_set_global_error() {
    let first = init(LevelFilter::INFO, LogFormat::Text);
    assert!(first.is_ok());

    let second = init(LevelFilter::INFO, LogFormat::Text);
    assert!(second.is_err());
}
