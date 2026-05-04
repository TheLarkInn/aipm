//! Engine API schema source-of-truth for AIPM.
//!
//! This crate hosts the canonical Rust types for the engine API schema and the
//! build-time generated tables (engines, tools, hook events, paths,
//! constraints) consumed by the rest of the workspace.
//!
//! The data file at `data/engine-api-schema.json` is treated as the single
//! source of truth: `build.rs` validates it against `schemas/engine-api.schema.json`,
//! parses it, and emits typed const tables into `OUT_DIR/engine_data.rs`.

pub mod generated;
pub mod helpers;
pub mod types;

pub use types::META_SCHEMA_VERSION;
