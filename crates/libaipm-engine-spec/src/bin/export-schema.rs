//! Exports the engine API JSON Schema from canonical Rust types.
//!
//! Implementation lands in feature #3 of the engine-api-schema
//! source-of-truth refactor: this binary will derive the schema via
//! `schemars::schema_for!(EngineApiSchemaFile)` and write it to
//! `schemas/engine-api.schema.json` at the workspace root.
fn main() {}
