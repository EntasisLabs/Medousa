//! Tauri host adapter for the undertaking binding application core.

use std::sync::Mutex;

use medousa_app_core::{
    UndertakingBindingCommand, UndertakingBindingCore, UndertakingBindingCoreError,
    UndertakingBindingUpdate,
};
use tauri::State;

#[derive(Default)]
pub struct AppUndertakingBindingState {
    core: Mutex<UndertakingBindingCore>,
}

fn map_error(error: UndertakingBindingCoreError) -> String {
    error.to_string()
}

#[tauri::command]
pub fn app_undertaking_binding_dispatch(
    state: State<'_, AppUndertakingBindingState>,
    command: UndertakingBindingCommand,
) -> Result<UndertakingBindingUpdate, String> {
    let mut core = state.core.lock().map_err(|error| error.to_string())?;
    core.dispatch(command).map_err(map_error)
}

#[tauri::command]
pub fn app_undertaking_binding_snapshot(
    state: State<'_, AppUndertakingBindingState>,
) -> Result<UndertakingBindingUpdate, String> {
    app_undertaking_binding_dispatch(state, UndertakingBindingCommand::Snapshot)
}
