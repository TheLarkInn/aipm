//! Centralised JSON generation for AI plugin artifacts.
//!
//! Replaces scattered generation code in `workspace_init`, `migrate/emitter`,
//! and `migrate/registrar` with a single canonical implementation per artifact.

pub mod marketplace;
pub mod plugin_json;
pub mod settings;
