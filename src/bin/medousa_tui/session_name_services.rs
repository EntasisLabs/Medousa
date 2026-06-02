use medousa::session::{get_session_display_name, set_session_display_name};

use super::daemon_commands::daemon_set_session_display_name;
use super::{TuiState, push_obs};

pub(crate) async fn set_session_display_name_daemon_first(
    state: &mut TuiState,
    display_name: &str,
) -> Result<(), String> {
    let session_id = state.session_id.clone();
    if !state.local_runtime_only {
        match daemon_set_session_display_name(&state.daemon_url, &session_id, display_name).await {
            Ok(response) => {
                state.session_display_name = Some(response.display_name);
                return Ok(());
            }
            Err(err) => {
                push_obs(
                    state,
                    format!(
                        "◈ session name daemon_error={} — trying local store",
                        truncate_error(&err.to_string(), 120)
                    ),
                );
            }
        }
    }

    set_session_display_name(&session_id, display_name)?;
    state.session_display_name = get_session_display_name(&session_id);
    Ok(())
}

pub(crate) fn refresh_session_display_name(state: &mut TuiState) {
    state.session_display_name = get_session_display_name(&state.session_id);
}

fn truncate_error(value: &str, max_chars: usize) -> String {
    let out = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        format!("{out}...")
    } else {
        out
    }
}
