use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use stasis::domain::errors::{Result as StasisResult, StasisError};

use super::ToolId;
use crate::tool_error::ToolError;

pub fn deserialize_input<T: DeserializeOwned>(tool_id: ToolId, input: Value) -> StasisResult<T> {
    serde_json::from_value(input).map_err(|error| {
        ToolError::input(tool_id.as_str(), format!("invalid input: {error}")).into()
    })
}

pub fn serialize_output<T: Serialize>(tool_id: ToolId, output: T) -> StasisResult<Value> {
    serde_json::to_value(output).map_err(|error| {
        StasisError::PortFailure(format!(
            "failed to serialize output for typed tool `{tool_id}`: {error}"
        ))
    })
}
