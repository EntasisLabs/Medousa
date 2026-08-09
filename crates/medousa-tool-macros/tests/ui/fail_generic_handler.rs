mod support;
pub use support::{stasis, typed_tools};

use medousa_tool_macros::medousa_tool;

struct GenericHandler;

#[medousa_tool(id = "generic_handler")]
impl GenericHandler {
    /// Generic handlers do not project one input contract.
    async fn invoke_typed<T>(&self, input: T) -> Result<T, ()> {
        Ok(input)
    }
}

fn main() {}
