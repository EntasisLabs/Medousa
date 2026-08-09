mod support;
pub use support::{stasis, typed_tools};

use medousa_tool_macros::medousa_tool;

struct MissingHandler;

#[medousa_tool(id = "missing_handler")]
impl MissingHandler {}

fn main() {}
