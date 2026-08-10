mod support;
pub use support::{stasis, typed_tools};

use medousa_tool_macros::medousa_tool;

struct MissingInput;

#[medousa_tool(id = "missing_input")]
impl MissingInput {
    /// A typed input is required.
    async fn invoke_typed(&self) -> Result<String, ()> {
        Ok(String::new())
    }
}

fn main() {}
