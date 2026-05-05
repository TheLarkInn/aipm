//! Tool adaptors for `aipm init`.
//!
//! Each adaptor integrates aipm's `.ai/` marketplace with a specific AI coding
//! tool by writing or merging tool-specific configuration files.

pub mod claude;
pub mod copilot;

use super::ToolAdaptor;
/// Returns the default set of tool adaptors to apply during init.
///
/// Order matches `Engine::ALL` in `libaipm-engine-spec`. The scaffold-set
/// filter in [`super::init`] selects which of these actually run based on
/// `Options.engines_scaffold`.
pub fn defaults() -> Vec<Box<dyn ToolAdaptor>> {
    vec![Box::new(claude::Adaptor), Box::new(copilot::Adaptor)]
}
