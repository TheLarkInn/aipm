//! Scaffolding command — `aipm make plugin`.
//!
//! This module composes the existing atomic CRUD primitives in
//! `generate/`, `manifest/`, and `init` into an ordered, idempotent
//! action pipeline exposed through the `aipm make` CLI command.

pub mod action;
pub mod engine_features;
pub mod error;
pub mod templates;

pub use action::Action;
pub use engine_features::Feature;
pub use error::Error;
