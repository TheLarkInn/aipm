//! Regenerates `schemas/engine-api.schema.json` from `src/types.rs`.
//!
//! Run manually whenever the canonical Rust types change in a way that
//! affects the on-the-wire shape:
//!
//! ```text
//! cargo run -p libaipm-engine-spec --bin export-schema
//! ```
//!
//! The output path is resolved relative to the crate's `CARGO_MANIFEST_DIR`
//! so the binary works no matter which directory you invoke `cargo run` from.

use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let schema = schemars::schema_for!(libaipm_engine_spec::EngineApiSchemaFile);
    let json = serde_json::to_string_pretty(&schema)?;

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let out_path = Path::new(manifest_dir).join("../../schemas/engine-api.schema.json");

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&out_path, json)?;
    Ok(())
}
