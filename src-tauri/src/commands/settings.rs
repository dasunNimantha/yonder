use std::sync::Mutex;

use tauri::{AppHandle, State};

use crate::config::{self, Settings};
use crate::discovery::Discovery;
use crate::identity::Identity;
use crate::state::AppState;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    discovery: State<'_, Mutex<Option<Discovery>>>,
    new_settings: Settings,
) -> Result<Settings, String> {
    let prev = state.settings();
    let merged = Settings {
        // device id must stay stable; ignore whatever the frontend sent.
        device_id: prev.device_id.clone(),
        ..new_settings
    };

    config::save(&merged).map_err(|e| format!("save failed: {e}"))?;
    state.set_settings(merged.clone());

    // Update identity TXT records if name changed.
    if merged.display_name != prev.display_name {
        state.set_identity(Identity::new(
            merged.device_id.clone(),
            Some(merged.display_name.clone()),
        ));
        if let Ok(guard) = discovery.lock() {
            if let Some(d) = guard.as_ref() {
                if let Err(e) = d.republish(&app, merged.tcp_port) {
                    log::warn!("mDNS re-publish failed: {e}");
                }
            }
        }
    }
    Ok(merged)
}
