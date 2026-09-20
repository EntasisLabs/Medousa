//! Read-only owner inventory across local and authorized connected workshops.

use crate::typed_tools::{ToolId, medousa_tool};
use stasis::prelude::{Result, StasisError};

pub const COGNITION_ACTIVE_WORK_DISCOVER: &str = "cognition_active_work_discover";
const ACTIVE_WORK_DISCOVER_ID: ToolId = ToolId::new(COGNITION_ACTIVE_WORK_DISCOVER);

pub fn register_active_work_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    delegation: Option<std::sync::Arc<crate::delegation::DelegationService>>,
) -> Result<()> {
    registry.register_typed_tool(ActiveWorkDiscoverTool { delegation })?;
    Ok(())
}

fn error(error: impl std::fmt::Display) -> StasisError {
    StasisError::PortFailure(error.to_string())
}

fn admitted()
-> Result<std::sync::Arc<crate::agent_runtime::execution_context::TurnExecutionContext>> {
    crate::agent_runtime::execution_context::active_turn_execution_context()
        .ok_or_else(|| error("active work discovery requires an admitted owner turn"))
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ActiveWorkDiscoverInput {
    /// Include accepted, discarded, and failed Forge work.
    #[serde(default)]
    include_terminal: bool,
}

struct ActiveWorkDiscoverTool {
    delegation: Option<std::sync::Arc<crate::delegation::DelegationService>>,
}

#[medousa_tool(id = ACTIVE_WORK_DISCOVER_ID)]
impl ActiveWorkDiscoverTool {
    /// List projects and agent sessions visible to the user across the local and authorized connected workshops. Use this for “what are we working on?” before asking the user to identify a project. The response is read-only and preserves unavailable workshops explicitly; never describe partial coverage as complete.
    async fn invoke_typed(&self, input: ActiveWorkDiscoverInput) -> Result<serde_json::Value> {
        let turn = admitted()?;
        let mut workshops = Vec::new();
        #[cfg(feature = "full-daemon")]
        if let Some(host) = crate::daemon::coordination::local_coordination_host() {
            let inventory = host
                .active_work_for_turn(turn.principal(), input.include_terminal)
                .await
                .map_err(error)?;
            workshops.push(serde_json::json!({
                "available": true,
                "target": {
                    "execution_runtime_id": inventory["coverage"]["execution_runtime_id"],
                    "authority_id": inventory["coverage"]["authority_id"],
                    "label": "Current workshop"
                },
                "inventory": inventory,
            }));
        }
        #[cfg(not(feature = "full-daemon"))]
        let _ = &turn;
        if let Some(delegation) = &self.delegation {
            workshops.extend(
                delegation
                    .active_work_inventories(input.include_terminal)
                    .await
                    .map_err(error)?,
            );
        }
        if workshops.is_empty() {
            return Err(error(
                "active work discovery has no local or connected workshops",
            ));
        }
        let complete_mesh = workshops.iter().all(|row| {
            row.get("available")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        });
        Ok(serde_json::json!({
            "coverage": {"kind": "authorized_mesh", "complete_mesh": complete_mesh},
            "workshops": workshops,
            "policy": "Read-only inventory. Unavailable workshops remain explicit and are never treated as empty."
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn active_work_discovery_requires_an_admitted_owner_turn() {
        assert!(
            ActiveWorkDiscoverTool { delegation: None }
                .invoke_typed(ActiveWorkDiscoverInput {
                    include_terminal: false,
                })
                .await
                .unwrap_err()
                .to_string()
                .contains("admitted owner turn")
        );
    }
}
