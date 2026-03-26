//! Installer module — orchestrates the install pipeline.
//!
//! The pipeline: resolve → fetch → store → link → lockfile.
//! This module is built incrementally as features land.

pub mod error;
pub mod manifest_editor;
