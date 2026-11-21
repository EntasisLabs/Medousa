use super::TuiState;
use super::daemon_commands::daemon_set_session_display_name;

pub(crate) async fn set_session_display_name_daemon_first(
    state: &mut TuiState,
    display_name: &str,
) -> Result<(), String> {
    let response =
        daemon_set_session_display_name(&state.daemon_url, &state.session_id, display_name)
            .await
            .map_err(|err| err.to_string())?;
    state.session_display_name = Some(response.display_name);
    Ok(())
}

pub(crate) fn refresh_session_display_name(state: &mut TuiState) {
    state.session_display_name = state
        .history_items
        .iter()
        .find(|s| s.session_id == state.session_id)
        .and_then(|s| s.display_name.clone());
}
