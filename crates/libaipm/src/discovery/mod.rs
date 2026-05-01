//! Unified feature discovery for the `aipm migrate` and `aipm lint` pipelines.
//!
//! This module is being built incrementally per the spec at
//! `specs/2026-05-01-unified-discovery-and-copilot-skill-detection.md`. In this
//! initial step it provides:
//!
//! - The new foundation types (`Engine`, `Layout`, the new `DiscoveredFeature`,
//!   `DiscoveredSet`, `ScanCounts`, `SkipReason`) in submodules `types` and
//!   `scan_report`. The `types` module name follows the existing codebase
//!   convention (e.g. `manifest/types.rs`) and avoids the
//!   `clippy::module_name_repetitions` trigger.
//! - Re-exports of the legacy types and free functions still in use by `lint`
//!   and `migrate` (`Error`, the legacy `DiscoveredFeature`, `SourceContext`,
//!   `DiscoveredSource`, `discover_features`, `discover_source_dirs`,
//!   `discover_claude_dirs`) so that today's call sites continue to compile
//!   unchanged.
//!
//! Callers needing the new shape should import via
//! `crate::discovery::types::DiscoveredFeature`. The legacy struct exposed at
//! `crate::discovery::DiscoveredFeature` will be removed once the lint and
//! migrate pipelines are switched over in later spec features.

pub mod instruction;
pub mod layout;
pub mod scan_report;
pub mod source;
pub mod types;

// New foundation types — accessible through both the submodule and the
// `types::` / `scan_report::` paths and re-exported here for convenience.
pub use instruction::classify as classify_instruction;
pub use layout::{
    match_agent, match_hook, match_marketplace, match_plugin, match_plugin_json, match_skill,
};
pub use scan_report::{DiscoveredSet, ScanCounts, SkipReason};
pub use source::infer_engine_root;
pub use types::{Engine, Layout};

// Re-exports from the legacy module so existing call sites
// (`crate::discovery::Error`, `crate::discovery::DiscoveredFeature`, …) keep
// resolving to their original types during the incremental migration.
pub use crate::discovery_legacy::{
    discover_claude_dirs, discover_features, discover_source_dirs, DiscoveredFeature,
    DiscoveredSource, Error, SourceContext,
};

// `FeatureKind` lives in the new `types` submodule but is re-exported here
// so existing call sites that say `crate::discovery::FeatureKind` keep working.
pub use types::FeatureKind;
