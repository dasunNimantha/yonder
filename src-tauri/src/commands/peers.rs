use tauri::State;

use crate::identity::Identity;
use crate::state::{AppState, Peer};

#[tauri::command]
pub fn list_peers(state: State<'_, AppState>) -> Vec<Peer> {
    state.list_peers()
}

#[tauri::command]
pub fn get_self(state: State<'_, AppState>) -> Identity {
    state.identity()
}
