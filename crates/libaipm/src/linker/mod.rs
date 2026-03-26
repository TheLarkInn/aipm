//! Linker module — manages symlinks, hard-links, gitignore, and link state.
//!
//! The linker implements a three-tier linking pipeline:
//! 1. Content store → `.aipm/links/{pkg}/` (hard-links)
//! 2. `.aipm/links/{pkg}/` → `claude-plugins/{pkg}/` (symlink/junction)
//!
//! This module is built incrementally — submodules are added as features land.

pub mod directory_link;
pub mod error;
pub mod gitignore;
pub mod hard_link;
pub mod link_state;
pub mod pipeline;
pub mod security;
