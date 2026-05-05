//! Detects drift between the committed `schemas/engine-api.schema.json`
//! and the schemars-derived schema for [`EngineApiSchemaFile`].
//!
//! When this test fails, the committed schema is stale: regenerate it
//! with `cargo run -p libaipm-engine-spec --bin export-schema`.

use std::path::Path;

#[test]
fn committed_schema_matches_schemars_export() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let committed_path = Path::new(manifest_dir).join("../../schemas/engine-api.schema.json");
    let committed_text =
        std::fs::read_to_string(&committed_path).expect("committed schema must exist");

    let derived = schemars::schema_for!(libaipm_engine_spec::EngineApiSchemaFile);
    let derived_text = serde_json::to_string_pretty(&derived).expect("serialize derived schema");

    let committed_value: serde_json::Value =
        serde_json::from_str(&committed_text).expect("parse committed schema");
    let derived_value: serde_json::Value =
        serde_json::from_str(&derived_text).expect("parse derived schema");

    assert_eq!(
        committed_value, derived_value,
        "schemas/engine-api.schema.json is out of date — re-run \
         `cargo run -p libaipm-engine-spec --bin export-schema`"
    );
}
