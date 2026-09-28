//! Verifies that calling [`libaipm::logging::init`] twice in the same
//! process fails on the second call, exercising the `try_init()` error
//! branch (`Error::SetGlobal`) in `logging.rs`.
//!
//! This lives in its own integration test binary (a separate process) so it
//! does not race with other tests that install a tracing subscriber (e.g.
//! `#[tracing_test::traced_test]` in `workspace_init`), which would otherwise
//! poison the global default and make outcomes order-dependent.

use libaipm::logging::{init, Error, LogFormat};
use tracing_subscriber::filter::LevelFilter;

#[test]
fn init_twice_returns_set_global_error() {
    let first = init(LevelFilter::WARN, LogFormat::Text);
    assert!(first.is_ok(), "first init call should succeed: {first:?}");

    let second = init(LevelFilter::WARN, LogFormat::Text);
    assert!(
        matches!(second, Err(Error::SetGlobal { .. })),
        "second init call in the same process must fail with SetGlobal: {second:?}"
    );
}
