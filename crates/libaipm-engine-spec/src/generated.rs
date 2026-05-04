//! Build-script generated tables.
//!
//! `build.rs` writes typed const tables to `OUT_DIR/engine_data.rs`
//! (`Engine` + `EngineSet` today; `ENGINES`, `VALID_TOOLS`,
//! `TOOL_COMPATIBILITY`, `HOOK_EVENTS_BY_ENGINE`, `paths`, `constraints`
//! in later features). This module simply `include!`s that file so its
//! items become part of `crate::generated`.

include!(concat!(env!("OUT_DIR"), "/engine_data.rs"));
