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

pub mod agent;
pub mod hook;
pub mod skill;

pub use agent::{ClaudeAgentAdapter, CopilotAgentAdapter};
pub use hook::CopilotHookAdapter;
pub use skill::{ClaudeSkillAdapter, CopilotSkillAdapter};

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
/// Order is engine then kind: Copilot adapters first (most-specific
/// `.github/copilot/...` shapes), then Claude adapters. Within an engine,
/// adapters are listed by kind (Skill, Agent, Hook). Adapters for the
/// remaining migrate kinds (MCP / Extension / LSP / Command / `OutputStyle`)
/// land in a follow-up feature once the unified discovery learns to
/// classify them — those kinds have no `FeatureKind` variants today.
#[must_use]
pub fn all() -> Vec<Box<dyn Adapter>> {
    vec![
        Box::new(CopilotSkillAdapter),
        Box::new(CopilotAgentAdapter),
        Box::new(CopilotHookAdapter),
        Box::new(ClaudeSkillAdapter),
        Box::new(ClaudeAgentAdapter),
    ]
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
        // Currently 5 adapters: Copilot (skill, agent, hook) + Claude
        // (skill, agent). Claude hook needs `.claude/settings.json`
        // discovery support and is deferred. MCP/Extension/LSP/Command/
        // OutputStyle have no FeatureKind variants and are also deferred.
        assert_eq!(registry.len(), 5);
    }

    #[test]
    fn registry_names_are_stable_and_ordered() {
        // First-match-wins resolution depends on order being deterministic
        // and well-known to the orchestrator + tests.
        let registry = all();
        let names: Vec<&str> = registry.iter().map(|a| a.name()).collect();
        assert_eq!(
            names,
            vec!["copilot-skill", "copilot-agent", "copilot-hook", "claude-skill", "claude-agent"]
        );
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
