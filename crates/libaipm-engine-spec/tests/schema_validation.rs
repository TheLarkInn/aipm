//! Asserts that `data/engine-api-schema.json` validates against the
//! committed `schemas/engine-api.schema.json` JSON Schema and that the
//! data file deserialises cleanly into the canonical Rust types.
//!
//! Mirrors the per-build validation already performed by `build.rs`,
//! but as a regular `cargo test` so CI surfaces the failure clearly
//! without forcing a recompile.

use std::path::Path;

#[test]
fn data_file_validates_against_committed_meta_schema() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let meta_schema_path = Path::new(manifest_dir).join("../../schemas/engine-api.schema.json");
    let data_path = Path::new(manifest_dir).join("data/engine-api-schema.json");

    let meta_schema_text = std::fs::read_to_string(&meta_schema_path).expect("read meta schema");
    let data_text = std::fs::read_to_string(&data_path).expect("read data file");

    let meta_schema: serde_json::Value =
        serde_json::from_str(&meta_schema_text).expect("parse meta schema");
    let data: serde_json::Value = serde_json::from_str(&data_text).expect("parse data file");

    let validator = jsonschema::validator_for(&meta_schema).expect("build validator");
    validator.validate(&data).expect(
        "data/engine-api-schema.json fails meta-schema validation; \
         re-run `cargo run -p libaipm-engine-spec --bin export-schema` if types changed",
    );
}

#[test]
fn data_file_round_trips_through_engine_api_schema_file() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let data_path = Path::new(manifest_dir).join("data/engine-api-schema.json");
    let data_text = std::fs::read_to_string(&data_path).expect("read data file");

    let parsed: libaipm_engine_spec::EngineApiSchemaFile =
        serde_json::from_str(&data_text).expect("data deserialises into EngineApiSchemaFile");
    assert_eq!(parsed.meta_schema_version, libaipm_engine_spec::META_SCHEMA_VERSION);
    assert!(!parsed.engines.is_empty());
}
