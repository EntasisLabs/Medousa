mod support;
pub use support::{stasis, typed_tools};

use medousa_tool_macros::medousa_tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const STATEFUL_TOOL_ID: typed_tools::ToolId = typed_tools::ToolId::new("stateful_tool");
const LEGACY_TOOL_ID: &str = "legacy_tool";

#[derive(Deserialize, JsonSchema)]
struct Input {
    value: String,
}

#[derive(Serialize, JsonSchema)]
struct Output {
    value: String,
}

struct StatefulTool {
    prefix: String,
}

#[medousa_tool(id = STATEFUL_TOOL_ID)]
impl StatefulTool {
    /// Prefix one typed value using constructor-owned state.
    async fn invoke_typed(&self, input: Input) -> stasis::prelude::Result<Output> {
        Ok(Output {
            value: format!("{}{}", self.prefix, input.value),
        })
    }
}

struct LegacyTool;

#[medousa_tool(id = LEGACY_TOOL_ID, crate_path = "crate::typed_tools")]
impl LegacyTool {
    /// Accept a legacy static string id during migration.
    async fn invoke_typed(&self, input: Input) -> stasis::prelude::Result<Output> {
        Ok(Output { value: input.value })
    }
}

fn assert_stasis_tool<T: stasis::application::orchestration::tool_registry::StasisTool>() {}

fn main() {
    assert_stasis_tool::<StatefulTool>();
    assert_stasis_tool::<LegacyTool>();
}
