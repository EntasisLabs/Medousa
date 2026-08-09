mod support;
pub use support::{stasis, typed_tools};

use medousa_tool_macros::medousa_tool;

struct NonAsync;

#[medousa_tool(id = "non_async")]
impl NonAsync {
    /// This handler should be rejected before expansion.
    fn invoke_typed(&self, input: String) -> Result<String, ()> {
        Ok(input)
    }
}

fn main() {}
