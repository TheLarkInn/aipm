//! Regression test for the `copilot-cli` -> `copilot` engine rename
//! (issue #724, spec G6).
//!
//! Locks in the rename so a future schema regen can't silently revert it.

use libaipm_engine_spec::Engine;

#[test]
fn engine_all_contains_exactly_claude_and_copilot() {
    let names: Vec<&str> = Engine::ALL.iter().map(|e| e.name()).collect();
    assert_eq!(names.len(), 2, "expected exactly 2 engines, got {names:?}");
    assert!(names.contains(&"claude"), "missing claude in {names:?}");
    assert!(names.contains(&"copilot"), "missing copilot in {names:?}");
}

#[test]
fn legacy_copilot_cli_name_is_no_longer_accepted() {
    // The deserializer used to accept "copilot-cli" pre-rename. After G6
    // there is no alias; only the canonical "copilot" form parses.
    assert_eq!(Engine::from_name("copilot-cli"), None);
    assert_eq!(Engine::from_name("copilot"), Some(Engine::Copilot));
    assert_eq!(Engine::from_name("claude"), Some(Engine::Claude));
}

#[test]
fn data_file_contains_no_copilot_cli_string() {
    // The bundled data file (read at build time) must not contain the
    // legacy "copilot-cli" string anywhere outside of npm package paths
    // (which use `@github/copilot`, no hyphen-cli suffix).
    let data = include_str!("../data/engine-api-schema.json");
    assert!(
        !data.contains("\"copilot-cli\""),
        "data file still contains the legacy \"copilot-cli\" string literal"
    );
    // Note: feature flag identifiers like `copilot_cli_skills_instructions`
    // are upstream binary names (extracted by reverse-binary-analysis) — they
    // are NOT engine identifiers and intentionally remain unchanged.
}
