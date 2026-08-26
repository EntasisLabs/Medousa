//! Shared projection from canonical UI tool output into turn-stream events.

/// Extract a renderable artifact from a successful UI tool result.
pub fn ui_artifact_from_tool_output(
    tool_output: &serde_json::Value,
) -> Option<crate::daemon_api::StreamUiArtifact> {
    if tool_output.get("ok").and_then(|value| value.as_bool()) != Some(true) {
        return None;
    }
    let artifact_id = tool_output
        .get("artifact_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let label = tool_output
        .get("label")
        .and_then(|value| value.as_str())
        .or_else(|| tool_output.get("title").and_then(|value| value.as_str()))
        .unwrap_or("Artifact")
        .to_string();
    let mime = tool_output
        .get("mime")
        .and_then(|value| value.as_str())
        .unwrap_or("text/html")
        .to_string();
    let presentation = tool_output
        .get("presentation")
        .and_then(|value| value.as_str())
        .unwrap_or("inline")
        .to_string();
    let byte_size = tool_output
        .get("byte_size")
        .and_then(|value| value.as_u64());
    let height_px = tool_output
        .get("height_px")
        .or_else(|| tool_output.get("height"))
        .and_then(|value| value.as_u64())
        .map(|value| value as u32);
    Some(crate::daemon_api::StreamUiArtifact {
        artifact_id,
        mime,
        label,
        presentation,
        byte_size,
        height_px,
    })
}

/// Extract a Liquid UI scene batch from a successful UI tool result.
pub fn scene_ops_from_tool_output(
    tool_output: &serde_json::Value,
) -> Option<crate::daemon_api::StreamUiScene> {
    if tool_output.get("ok").and_then(|value| value.as_bool()) != Some(true) {
        return None;
    }
    let ops = tool_output
        .get("ops")
        .and_then(|value| value.as_array())
        .filter(|ops| !ops.is_empty())?
        .clone();
    let surface_id = tool_output
        .get("surface_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let rev = tool_output.get("rev").and_then(|value| value.as_i64());
    Some(crate::daemon_api::StreamUiScene {
        turn_id: None,
        surface_id,
        rev,
        ops,
    })
}
