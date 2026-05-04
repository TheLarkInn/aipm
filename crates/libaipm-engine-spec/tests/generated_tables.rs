//! Smoke tests over the build-script-generated const tables.
//!
//! These run from outside the crate (as integration tests) so they
//! exercise the full public API surface as a downstream consumer would.

use libaipm_engine_spec::{
    Engine, EngineSet, ENGINES, HOOK_EVENTS_BY_ENGINE, TOOL_COMPATIBILITY, VALID_TOOLS,
};

#[test]
fn engine_all_has_at_least_two_entries() {
    assert!(Engine::ALL.len() >= 2, "expected ≥ 2 engines, got {}", Engine::ALL.len());
}

#[test]
fn engines_const_aligns_with_engine_all() {
    assert_eq!(ENGINES.len(), Engine::ALL.len());
    let names: Vec<&str> = ENGINES.iter().map(|(_, spec)| spec.name).collect();
    for engine in Engine::ALL {
        assert!(names.contains(&engine.name()), "ENGINES missing {}", engine.name());
    }
}

#[test]
fn valid_tools_contains_canonical_names_and_aliases() {
    assert!(!VALID_TOOLS.is_empty());
    assert!(VALID_TOOLS.contains("bash"), "expected canonical lowercase 'bash'");
    assert!(VALID_TOOLS.contains("Bash"), "expected claude-style 'Bash'");
}

#[test]
fn tool_compatibility_non_empty_with_non_empty_support_sets() {
    assert!(!TOOL_COMPATIBILITY.is_empty());
    for (tool, set) in TOOL_COMPATIBILITY {
        assert!(!set.is_empty(), "tool {tool} has empty support set");
        assert!(set.bits() & EngineSet::ALL.bits() == set.bits(), "tool {tool} has unknown bits");
    }
}

#[test]
fn hook_events_by_engine_has_entry_for_each_engine_variant() {
    for &engine in Engine::ALL {
        let entry = HOOK_EVENTS_BY_ENGINE.iter().find(|(e, _)| *e == engine);
        assert!(entry.is_some(), "HOOK_EVENTS_BY_ENGINE missing {engine:?}");
        let events = entry.expect("checked above").1;
        assert!(!events.is_empty(), "{engine:?} has zero hook events");
    }
}
