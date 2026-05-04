//! Build script for `libaipm-engine-spec`.
//!
//! Today this validates `data/engine-api-schema.json` against
//! `schemas/engine-api.schema.json` and confirms the data file's
//! `meta_schema_version` matches `types::META_SCHEMA_VERSION`.
//!
//! Later features will extend this to emit typed const tables
//! (`Engine`, `EngineSet`, `ENGINES`, `VALID_TOOLS`,
//! `TOOL_COMPATIBILITY`, `HOOK_EVENTS_BY_ENGINE`, `paths`,
//! `constraints`) into `OUT_DIR/engine_data.rs`.
//!
//! `println!` is denied workspace-wide, so cargo directives are
//! emitted via `writeln!(io::stdout(), …)` instead.

use std::io::Write;

#[path = "src/types.rs"]
mod types;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    emit_rerun_directives()?;

    let meta_schema_text = std::fs::read_to_string("../../schemas/engine-api.schema.json")?;
    let data_text = std::fs::read_to_string("data/engine-api-schema.json")?;

    let meta_schema: serde_json::Value = serde_json::from_str(&meta_schema_text)?;
    let data: serde_json::Value = serde_json::from_str(&data_text)?;

    let validator = jsonschema::validator_for(&meta_schema)?;
    if let Err(e) = validator.validate(&data) {
        return Err(format!(
            "data/engine-api-schema.json fails meta-schema validation: {e}\n\
             If the meta-schema needs updating, edit src/types.rs and re-run \
             `cargo run -p libaipm-engine-spec --bin export-schema`."
        )
        .into());
    }

    let parsed: types::EngineApiSchemaFile = serde_json::from_value(data)?;

    if parsed.meta_schema_version != types::META_SCHEMA_VERSION {
        return Err(format!(
            "meta_schema_version mismatch: data file says {data_v} but src/types.rs says {types_v}",
            data_v = parsed.meta_schema_version,
            types_v = types::META_SCHEMA_VERSION,
        )
        .into());
    }

    Ok(())
}

fn emit_rerun_directives() -> Result<(), std::io::Error> {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    for line in [
        "cargo:rerun-if-changed=data/engine-api-schema.json",
        "cargo:rerun-if-changed=../../schemas/engine-api.schema.json",
        "cargo:rerun-if-changed=src/types.rs",
        "cargo:rerun-if-changed=build.rs",
    ] {
        writeln!(handle, "{line}")?;
    }
    Ok(())
}
