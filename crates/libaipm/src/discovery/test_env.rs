//! Test-only helpers for serializing access to the
//! `AIPM_UNIFIED_DISCOVERY` env var across modules.
//!
//! `std::env::set_var` is process-global, and cargo runs tests in parallel.
//! Any test that toggles the env var must hold this single shared lock for
//! the duration of its scope. Both `discovery::tests` and
//! `lint::tests` reach for this helper to avoid races.

use std::sync::{Mutex, PoisonError};

use super::UNIFIED_DISCOVERY_ENV;

/// Process-wide lock guarding env-var manipulation in tests.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Run `body` with [`UNIFIED_DISCOVERY_ENV`] set to `value` (or unset if
/// `None`). Restores the previous value on completion. Holds [`ENV_LOCK`]
/// for the duration of `body` to serialize with peer tests.
pub(crate) fn with_unified_discovery_env<F: FnOnce()>(value: Option<&str>, body: F) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let prev = std::env::var(UNIFIED_DISCOVERY_ENV).ok();
    match value {
        Some(v) => std::env::set_var(UNIFIED_DISCOVERY_ENV, v),
        None => std::env::remove_var(UNIFIED_DISCOVERY_ENV),
    }
    body();
    match prev {
        Some(v) => std::env::set_var(UNIFIED_DISCOVERY_ENV, v),
        None => std::env::remove_var(UNIFIED_DISCOVERY_ENV),
    }
}
