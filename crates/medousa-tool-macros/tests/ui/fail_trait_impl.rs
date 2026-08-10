mod support;
pub use support::{stasis, typed_tools};

use medousa_tool_macros::medousa_tool;

trait Handler {
    fn marker(&self);
}

struct TraitTool;

#[medousa_tool(id = "trait_tool")]
impl Handler for TraitTool {
    fn marker(&self) {}
}

fn main() {}
