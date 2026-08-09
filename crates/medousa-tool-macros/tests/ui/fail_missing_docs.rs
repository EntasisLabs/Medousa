mod support;
pub use support::{stasis, typed_tools};

use medousa_tool_macros::medousa_tool;

struct MissingDocs;

#[medousa_tool(id = "missing_docs")]
impl MissingDocs {
    async fn invoke_typed(&self, input: String) -> Result<String, ()> {
        Ok(input)
    }
}

fn main() {}
