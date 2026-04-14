//! Scaffolding command — `aipm make plugin`.
//!
//! This module composes the existing atomic CRUD primitives in
//! `generate/`, `manifest/`, and `init` into an ordered, idempotent
//! action pipeline exposed through the `aipm make` CLI command.

pub mod action;
pub mod error;

pub use action::Action;
pub use error::Error;
