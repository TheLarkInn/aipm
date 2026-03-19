//! Tool adaptors for `aipm init`.
//!
//! Each adaptor integrates aipm's `.ai/` marketplace with a specific AI coding
//! tool by writing or merging tool-specific configuration files.

pub mod claude;

use super::ToolAdaptor;
/// Returns the default set of tool adaptors to apply during init.
///
/// Currently only includes Claude Code. Future adaptors (Copilot CLI,
/// `OpenCode`, etc.) are added here.
pub fn defaults() -> Vec<Box<dyn ToolAdaptor>> {
    vec![Box::new(claude::Adaptor)]
}
