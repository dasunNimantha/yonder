use iroh::endpoint::Endpoint;
use tauri::State;

use crate::config::{self, Settings};
use crate::identity::Identity;
use crate::net::{self, PeerUserDataIn};
use crate::state::AppState;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn update_settings(
    state: State<'_, AppState>,
    endpoint: State<'_, Endpoint>,
    new_settings: Settings,
) -> Result<Settings, String> {
    let prev = state.settings();
    let merged = Settings {
        // The secret key never changes via the frontend — preserve it
        // verbatim no matter what the UI sent.
        secret_key: prev.secret_key.clone(),
        ..new_settings
    };

    config::save(&merged).map_err(|e| format!("save failed: {e}"))?;
    state.set_settings(merged.clone());

    // If the display name changed, refresh the in-memory identity AND
    // re-publish the user_data over mDNS so other peers see the new
    // label.
    if merged.display_name != prev.display_name {
        let secret = merged
            .secret()
            .map_err(|e| format!("invalid secret key: {e}"))?;
        let identity = Identity::new(&secret, Some(merged.display_name.clone()));
        state.set_identity(identity.clone());
        if let Err(e) = net::republish_user_data(
            endpoint.inner(),
            &PeerUserDataIn {
                name: identity.name.clone(),
                os: identity.os.clone(),
                version: identity.version.clone(),
            },
        ) {
            log::warn!("user_data re-publish failed: {e}");
        }
    }
    Ok(merged)
}
