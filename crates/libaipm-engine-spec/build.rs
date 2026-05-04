//! Build script for `libaipm-engine-spec`.
//!
//! In later features this will:
//!   * validate `data/engine-api-schema.json` against `schemas/engine-api.schema.json`
//!   * parse the data file via `serde`
//!   * emit typed const tables (`Engine`, `EngineSet`, `ENGINES`, `VALID_TOOLS`,
//!     `TOOL_COMPATIBILITY`, `HOOK_EVENTS_BY_ENGINE`, `paths`, `constraints`)
//!     to `OUT_DIR/engine_data.rs`.
fn main() {}
