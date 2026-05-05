//! Drift test: ensure `schemas/aipm.toml.schema.json`'s `engines` enum
//! stays in sync with `libaipm_engine_spec::Engine::ALL`.
//!
//! Spec G8 / Feature 16 — when a new engine is added to the
//! schema-driven catalog (`engine-api-schema.json`), this test fails
//! until the JSON schema's `$defs.engineList.items.enum` is updated to
//! match. This prevents IDE autocomplete from silently lying about
//! valid engine names.

use libaipm_engine_spec::Engine;

#[test]
fn aipm_toml_schema_engine_enum_matches_engine_all() {
    let schema_text = std::fs::read_to_string("../../schemas/aipm.toml.schema.json")
        .expect("read aipm.toml schema");
    let schema: serde_json::Value =
        serde_json::from_str(&schema_text).expect("parse aipm.toml schema");

    let enum_values = schema
        .get("$defs")
        .and_then(|d| d.get("engineList"))
        .and_then(|el| el.get("items"))
        .and_then(|i| i.get("enum"))
        .and_then(|e| e.as_array())
        .expect("schema must define $defs.engineList.items.enum");

    let schema_names: Vec<&str> = enum_values.iter().filter_map(|v| v.as_str()).collect();
    let expected_names: Vec<&str> = Engine::ALL.iter().map(|e| e.name()).collect();

    assert_eq!(
        schema_names, expected_names,
        "JSON Schema engine enum is out of sync with Engine::ALL.\n\
         Update schemas/aipm.toml.schema.json $defs.engineList.items.enum to match\n\
         the kebab-case names returned by Engine::name() in libaipm-engine-spec."
    );
}

#[test]
fn aipm_toml_schema_exposes_engines_on_package_and_workspace() {
    // Sanity check that both [package].engines and [workspace].engines
    // surfaces are present and reference the shared $defs.engineList
    // (so they can never diverge).
    let schema_text = std::fs::read_to_string("../../schemas/aipm.toml.schema.json")
        .expect("read aipm.toml schema");
    let schema: serde_json::Value =
        serde_json::from_str(&schema_text).expect("parse aipm.toml schema");

    let package_ref = schema
        .pointer("/properties/package/properties/engines/$ref")
        .and_then(|v| v.as_str())
        .expect("[package].engines must be defined and use a $ref");
    assert_eq!(package_ref, "#/$defs/engineList");

    let workspace_ref = schema
        .pointer("/properties/workspace/properties/engines/$ref")
        .and_then(|v| v.as_str())
        .expect("[workspace].engines must be defined and use a $ref");
    assert_eq!(workspace_ref, "#/$defs/engineList");
}
