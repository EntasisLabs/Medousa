//! Incremental `#[stasis_tool]` migration for turn-control tools (parallel track).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use stasis_macros::stasis_tool;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TurnControlPingInput {
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TurnControlPingOutput {
    pub ok: bool,
    pub tool: &'static str,
    pub note: Option<String>,
}

#[stasis_tool(
    name = "cognition_turn_control_ping",
    description = "Typed-tool pilot for turn-control migration (no runtime effect)",
    crate_path = "stasis"
)]
async fn cognition_turn_control_ping(
    input: TurnControlPingInput,
) -> stasis::prelude::Result<TurnControlPingOutput> {
    Ok(TurnControlPingOutput {
        ok: true,
        tool: "cognition_turn_control_ping",
        note: input.note,
    })
}

pub fn register_turn_control_stasis_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
) -> stasis::prelude::Result<()> {
    registry.register_tool(cognition_turn_control_ping_tool())?;
    Ok(())
}
