//! Integration test for `libaipm::logging::init`'s `SetGlobal` error branch.
//!
//! Runs in its own test binary (separate process) so setting the global
//! tracing subscriber here cannot interfere with other unit tests that also
//! initialize a subscriber (e.g. `#[tracing_test::traced_test]` tests in
//! `libaipm::workspace_init`).

use tracing_subscriber::filter::LevelFilter;

/// Calling `init` a second time fails because the global tracing subscriber
/// is already set, exercising the `Error::SetGlobal` branch (`try_init()`
/// returning `Err`) in `libaipm::logging::init`.
#[test]
fn init_twice_returns_set_global_error() {
    let _ = libaipm::logging::init(LevelFilter::OFF, libaipm::logging::LogFormat::Text);
    let second = libaipm::logging::init(LevelFilter::OFF, libaipm::logging::LogFormat::Text);
    assert!(matches!(second, Err(libaipm::logging::Error::SetGlobal { .. })));
}
