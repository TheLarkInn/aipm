//! Integration test for `libaipm::logging::init`.
//!
//! Runs in its own process (separate from the unit test binary) so setting
//! the global tracing subscriber here cannot poison other tests that rely on
//! `tracing_test::traced_test`, which also installs a global subscriber.

use libaipm::logging::{init, Error, LogFormat};
use tracing_subscriber::filter::LevelFilter;

/// Calling `init` a second time in the same process must fail because the
/// global tracing subscriber can only be set once. This exercises the
/// `try_init` error branch that maps into `Error::SetGlobal`.
#[test]
fn init_called_twice_returns_set_global_error() {
    std::env::remove_var("AIPM_LOG");

    let first = init(LevelFilter::OFF, LogFormat::Text);
    assert!(first.is_ok(), "first init call should succeed: {first:?}");

    let second = init(LevelFilter::OFF, LogFormat::Text);
    assert!(second.is_err(), "second init call should fail: {second:?}");
    if let Err(e) = second {
        assert!(matches!(e, Error::SetGlobal { .. }));
        assert!(e.to_string().contains("global tracing subscriber"));
    }
}
