mod support;
pub use support::{stasis, typed_tools};

use medousa_tool_macros::medousa_tool;

struct PlainReturn;

#[medousa_tool(id = "plain_return")]
impl PlainReturn {
    /// Plain outputs cannot preserve Stasis errors.
    async fn invoke_typed(&self, input: String) -> String {
        input
    }
}

fn main() {}
