mod support;
pub use support::{stasis, typed_tools};

use medousa_tool_macros::medousa_tool;

struct MultipleHandlers;

#[medousa_tool(id = "multiple_handlers")]
impl MultipleHandlers {
    /// First handler.
    async fn invoke_typed(&self, input: String) -> Result<String, ()> {
        Ok(input)
    }

    /// Second handler.
    async fn invoke_typed(&self, input: String) -> Result<String, ()> {
        Ok(input)
    }
}

fn main() {}
