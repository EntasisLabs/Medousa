//! Portable, immutable context admitted for one daemon-owned turn.

use std::collections::BTreeSet;

use medousa_types::TurnWorldSelection;

pub const MAX_TURN_SELECTED_WORLDS: usize = 8;

/// Where the daemon delivers a turn or job result after execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelDeliveryTarget {
    pub channel: String,
    pub user_id: String,
    pub channel_id: String,
    pub session_id: String,
    pub stream_id: Option<String>,
}

impl ChannelDeliveryTarget {
    pub fn new(
        channel: impl Into<String>,
        user_id: impl Into<String>,
        channel_id: impl Into<String>,
        session_id: impl Into<String>,
        stream_id: Option<String>,
    ) -> Self {
        Self {
            channel: channel.into(),
            user_id: user_id.into(),
            channel_id: channel_id.into(),
            session_id: session_id.into(),
            stream_id,
        }
    }

    pub fn interactive(
        channel: impl Into<String>,
        user_id: impl Into<String>,
        channel_id: impl Into<String>,
        session_id: impl Into<String>,
        turn_id: impl Into<String>,
    ) -> Self {
        Self::new(
            channel,
            user_id,
            channel_id,
            session_id,
            Some(turn_id.into()),
        )
    }
}

/// Compatibility context retained by the production loop while callers move
/// to typed execution ports. Authority is fixed at daemon admission time.
#[derive(Debug, Clone)]
pub struct TurnContinuationScope {
    pub turn_correlation_id: String,
    pub session_id: String,
    pub identity_user_id: Option<String>,
    pub original_prompt: String,
    pub delivery_target: Option<ChannelDeliveryTarget>,
    pub provider: String,
    pub model: String,
    pub response_depth_mode: String,
    pub supports_ui_artifacts: bool,
    pub supports_liquid_markdown: bool,
    pub supports_browser_host: bool,
    pub browser_driver_id: Option<String>,
    pub selected_worlds: Vec<TurnWorldSelection>,
    pub channel_surface: Option<String>,
}

impl TurnContinuationScope {
    /// Model-safe view of the worlds admitted for this exact turn. Placement
    /// stays in runtime context; prompts and worker requests receive ids only.
    pub fn selected_world_ids(&self) -> Vec<String> {
        self.selected_worlds
            .iter()
            .map(|selection| selection.world_id.clone())
            .collect()
    }

    pub fn world_prompt_appendix(&self) -> Option<String> {
        let ids = self.selected_world_ids();
        if ids.is_empty() {
            return None;
        }
        Some(format!(
            "[MEDOUSA_ELIGIBLE_WORLDS]\n\
             These opaque world ids were selected by the operator for this turn. \
             Never invent an id or infer a driver, URL, host, or transport from it.\n- {}",
            ids.join("\n- ")
        ))
    }
}

pub fn normalize_turn_world_selections(
    selections: Vec<TurnWorldSelection>,
) -> Result<Vec<TurnWorldSelection>, String> {
    if selections.len() > MAX_TURN_SELECTED_WORLDS {
        return Err(format!(
            "a turn may select at most {MAX_TURN_SELECTED_WORLDS} worlds"
        ));
    }
    let mut normalized = Vec::with_capacity(selections.len());
    let mut seen = BTreeSet::new();
    for selection in selections {
        let world_id = normalize_world_id(&selection.world_id)?;
        let execution_runtime_id = normalize_runtime_id(&selection.execution_runtime_id)?;
        if !seen.insert(world_id.clone()) {
            return Err(format!("world '{world_id}' was selected more than once"));
        }
        normalized.push(TurnWorldSelection {
            world_id,
            execution_runtime_id,
        });
    }
    normalized.sort_by(|left, right| left.world_id.cmp(&right.world_id));
    Ok(normalized)
}

/// Resolve a model-authored subset against immutable user admission and exact
/// worker placement. The resulting wire value deliberately contains ids only.
pub fn resolve_requested_world_ids(
    requested: &[String],
    selected: &[TurnWorldSelection],
    resolved_runtime_id: &str,
) -> Result<Vec<String>, String> {
    if requested.len() > MAX_TURN_SELECTED_WORLDS {
        return Err(format!(
            "a worker may request at most {MAX_TURN_SELECTED_WORLDS} worlds"
        ));
    }
    let resolved_runtime_id = normalize_runtime_id(resolved_runtime_id)?;
    let mut ids = Vec::with_capacity(requested.len());
    let mut seen = BTreeSet::new();
    for requested_id in requested {
        let world_id = normalize_world_id(requested_id)?;
        if !seen.insert(world_id.clone()) {
            return Err(format!(
                "worker requested world '{world_id}' more than once"
            ));
        }
        let selection = selected
            .iter()
            .find(|selection| selection.world_id == world_id)
            .ok_or_else(|| format!("world '{world_id}' is not eligible for this turn"))?;
        if selection.execution_runtime_id != resolved_runtime_id {
            return Err(format!(
                "world '{world_id}' belongs to another execution runtime"
            ));
        }
        ids.push(world_id);
    }
    ids.sort();
    Ok(ids)
}

pub fn validate_world_ids(ids: &[String]) -> Result<(), String> {
    if ids.len() > MAX_TURN_SELECTED_WORLDS {
        return Err(format!(
            "a worker may request at most {MAX_TURN_SELECTED_WORLDS} worlds"
        ));
    }
    let mut previous: Option<&str> = None;
    for id in ids {
        let normalized = normalize_world_id(id)?;
        if normalized != *id {
            return Err("worker world ids must be normalized".to_string());
        }
        if previous.is_some_and(|value| value >= id.as_str()) {
            return Err("worker world ids must be sorted and unique".to_string());
        }
        previous = Some(id);
    }
    Ok(())
}

fn normalize_world_id(value: &str) -> Result<String, String> {
    normalize_opaque_id("world id", value, 1_024).and_then(|value| {
        if value.starts_with("world:") {
            Ok(value)
        } else {
            Err("world id has unsupported syntax".to_string())
        }
    })
}

fn normalize_runtime_id(value: &str) -> Result<String, String> {
    normalize_opaque_id("execution runtime id", value, 256)
}

fn normalize_opaque_id(label: &str, value: &str, max_len: usize) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > max_len
        || !value.bytes().all(|byte| matches!(byte, 0x21..=0x7e))
    {
        return Err(format!("{label} is invalid"));
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection(world_id: &str, runtime_id: &str) -> TurnWorldSelection {
        TurnWorldSelection {
            world_id: world_id.to_string(),
            execution_runtime_id: runtime_id.to_string(),
        }
    }

    #[test]
    fn selected_worlds_are_normalized_once_at_admission() {
        let worlds = normalize_turn_world_selections(vec![
            selection(" world:two ", " runtime-b "),
            selection("world:one", "runtime-a"),
        ])
        .unwrap();
        assert_eq!(worlds[0], selection("world:one", "runtime-a"));
        assert_eq!(worlds[1], selection("world:two", "runtime-b"));
        assert!(
            normalize_turn_world_selections(vec![
                selection("world:one", "runtime-a"),
                selection(" world:one ", "runtime-a"),
            ])
            .is_err()
        );
    }

    #[test]
    fn worker_gets_only_an_eligible_subset_on_its_runtime() {
        let selected = vec![
            selection("world:browser:a", "runtime-a"),
            selection("world:computer:b", "runtime-b"),
        ];
        assert_eq!(
            resolve_requested_world_ids(&["world:browser:a".to_string()], &selected, "runtime-a")
                .unwrap(),
            vec!["world:browser:a"]
        );
        assert!(
            resolve_requested_world_ids(&["world:computer:b".to_string()], &selected, "runtime-a")
                .is_err()
        );
        assert!(
            resolve_requested_world_ids(&["world:unknown".to_string()], &selected, "runtime-a")
                .is_err()
        );
        assert!(
            resolve_requested_world_ids(&[], &selected, "runtime-a")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn prompt_view_does_not_expose_placement() {
        let scope = TurnContinuationScope {
            turn_correlation_id: "turn-1".to_string(),
            session_id: "session-1".to_string(),
            identity_user_id: None,
            original_prompt: "hello".to_string(),
            delivery_target: None,
            provider: "provider".to_string(),
            model: "model".to_string(),
            response_depth_mode: "standard".to_string(),
            supports_ui_artifacts: false,
            supports_liquid_markdown: false,
            supports_browser_host: false,
            browser_driver_id: None,
            selected_worlds: vec![selection("world:browser:a", "secret-runtime")],
            channel_surface: None,
        };
        let prompt = scope.world_prompt_appendix().unwrap();
        assert!(prompt.contains("world:browser:a"));
        assert!(!prompt.contains("secret-runtime"));
    }
}
