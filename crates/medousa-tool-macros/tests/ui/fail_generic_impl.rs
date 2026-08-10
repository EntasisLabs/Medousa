mod support;
pub use support::{stasis, typed_tools};

use medousa_tool_macros::medousa_tool;

struct GenericTool<T>(T);

#[medousa_tool(id = "generic_tool")]
impl<T> GenericTool<T> {
    /// Generic tool impls do not project one static contract.
    async fn invoke_typed(&self, input: String) -> Result<String, ()> {
        Ok(input)
    }
}

fn main() {}
