mod support;
pub use support::{stasis, typed_tools};

use medousa_tool_macros::medousa_tool;

struct MutableReceiver;

#[medousa_tool(id = "mutable_receiver")]
impl MutableReceiver {
    /// Mutable receivers are incompatible with shared registry invocation.
    async fn invoke_typed(&mut self, input: String) -> Result<String, ()> {
        Ok(input)
    }
}

fn main() {}
