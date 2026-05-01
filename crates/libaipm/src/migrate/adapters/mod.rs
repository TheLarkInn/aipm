//! Adapter trait and registry for the unified migrate pipeline.
//!
//! Replaces the legacy `Detector` trait at
//! `crates/libaipm/src/migrate/detector.rs`. Where a `Detector` walked the
//! filesystem itself, an `Adapter` consumes a [`DiscoveredFeature`] (already
//! found by `discovery::discover`) and produces an [`Artifact`].
//!
//! # Resolution policy: first-match-wins
//!
//! Multiple adapters may technically claim the same [`DiscoveredFeature`]
//! (for example, if both a Claude and a Copilot adapter were registered for
//! the same kind). The migrate orchestrator MUST iterate
//! [`all`] in registry order and select the first adapter whose
//! [`Adapter::applies_to`] returns `true`. Implementations should make
//! `applies_to` precise enough that this never happens in practice — engine
//! + kind discriminate the 12 adapters cleanly.
//!
//! This module ships with an empty registry. Each engine-specific adapter
//! (Claude × 6, Copilot × 6) is added in subsequent spec features.

use crate::discovery::DiscoveredFeature;
use crate::fs::Fs;

use super::{Artifact, Error};

/// Translates a discovered feature into a migration artifact.
///
/// Adapters replace the legacy `Detector` trait. They are called by the
/// migrate orchestrator after `discovery::discover` has classified files,
/// so they do not perform their own filesystem walks.
pub trait Adapter: Send + Sync {
    /// Stable name for diagnostics, registry ordering, and tests.
    fn name(&self) -> &'static str;

    /// `true` if this adapter wants to handle `feat`.
    fn applies_to(&self, feat: &DiscoveredFeature) -> bool;

    /// Convert a discovered feature into a migration artifact.
    ///
    /// `fs` is provided so the adapter can read frontmatter, collect
    /// referenced files, etc. without re-walking the project tree.
    ///
    /// # Errors
    ///
    /// Returns the migrate `Error` if reading or parsing the feature
    /// fails.
    fn to_artifact(&self, feat: &DiscoveredFeature, fs: &dyn Fs) -> Result<Artifact, Error>;
}

/// Build the canonical adapter registry, in resolution order.
///
/// Currently empty — each engine-specific adapter lands in its own spec
/// feature. Tests assert the order is stable so the orchestrator can rely
/// on first-match-wins.
#[must_use]
pub fn all() -> Vec<Box<dyn Adapter>> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stub adapter for orderiing tests — never selected at runtime.
    struct StubAdapter;
    impl Adapter for StubAdapter {
        fn name(&self) -> &'static str {
            "stub"
        }
        fn applies_to(&self, _feat: &DiscoveredFeature) -> bool {
            false
        }
        fn to_artifact(&self, _feat: &DiscoveredFeature, _fs: &dyn Fs) -> Result<Artifact, Error> {
            Err(Error::UnsupportedSource("stub never produces artifacts".to_string()))
        }
    }

    #[test]
    fn registry_is_a_vec_of_box_dyn_adapter() {
        // Compile-time check via type inference: the function must return
        // exactly Vec<Box<dyn Adapter>>.
        let registry: Vec<Box<dyn Adapter>> = all();
        // The registry is intentionally empty until adapters land.
        assert_eq!(registry.len(), 0, "registry currently empty; adapters added in later features");
    }

    #[test]
    fn registry_iteration_is_safe_when_empty() {
        // The orchestrator iterates the registry; an empty registry should
        // simply yield nothing without panicking.
        let registry = all();
        let mut count = 0_usize;
        for _adapter in &registry {
            count += 1;
        }
        assert_eq!(count, 0);
    }

    #[test]
    fn stub_adapter_does_not_apply() {
        let stub = StubAdapter;
        let feat = DiscoveredFeature {
            kind: crate::discovery::FeatureKind::Skill,
            engine: crate::discovery::Engine::Claude,
            layout: crate::discovery::Layout::Canonical,
            source_root: std::path::PathBuf::from(".claude"),
            feature_dir: None,
            path: std::path::PathBuf::from(".claude/skills/x/SKILL.md"),
        };
        assert!(!stub.applies_to(&feat));
        assert_eq!(stub.name(), "stub");
    }

    #[test]
    fn adapter_trait_is_object_safe() {
        // Verify dyn Adapter compiles and the methods can be called via
        // dynamic dispatch — guards against accidental introduction of
        // generic methods that would break object safety.
        let stub: Box<dyn Adapter> = Box::new(StubAdapter);
        assert_eq!(stub.name(), "stub");
    }
}
