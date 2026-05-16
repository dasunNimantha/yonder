use std::path::PathBuf;

use chrono::Utc;
use tauri::{AppHandle, Emitter, State};

use crate::client;
use crate::state::{AppState, ApprovalDecision};
use crate::transfer::{Transfer, TransferStatus};

#[tauri::command]
pub fn list_transfers(state: State<'_, AppState>) -> Vec<Transfer> {
    state.list_transfers()
}

#[tauri::command]
pub fn send_files(
    app: AppHandle,
    state: State<'_, AppState>,
    peer_id: String,
    paths: Vec<String>,
) -> Result<String, String> {
    let peer = state
        .get_peer(&peer_id)
        .ok_or_else(|| format!("unknown peer id: {peer_id}"))?;
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    if paths.is_empty() {
        return Err("no files to send".into());
    }

    client::spawn_send(app, state.inner().clone(), peer, paths).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn accept_incoming(
    app: AppHandle,
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
    let sender = state
        .take_pending_approval(&transfer_id)
        .ok_or_else(|| "no pending approval for that transfer".to_string())?;
    sender
        .send(ApprovalDecision::Accept)
        .map_err(|_| "receiver dropped".to_string())?;
    let _ = app.emit("transfer-approved", &transfer_id);
    Ok(())
}

#[tauri::command]
pub fn reject_incoming(
    app: AppHandle,
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
    if let Some(sender) = state.take_pending_approval(&transfer_id) {
        let _ = sender.send(ApprovalDecision::Reject);
    }
    let _ = state.update_transfer(&transfer_id, |t| {
        t.status = TransferStatus::Rejected;
        t.finished_at = Some(Utc::now());
    });
    if let Some(t) = state.get_transfer(&transfer_id) {
        let _ = app.emit("transfer-finished", &t);
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_transfer(
    app: AppHandle,
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
    // For now "cancel" is a hard mark; the in-flight stream is not
    // aborted (would require carrying a CancellationToken per transfer
    // — a future improvement). The frontend will hide the row.
    let _ = state.update_transfer(&transfer_id, |t| {
        if matches!(
            t.status,
            TransferStatus::Active | TransferStatus::AwaitingApproval | TransferStatus::Pending
        ) {
            t.status = TransferStatus::Cancelled;
            t.finished_at = Some(Utc::now());
        }
    });
    if let Some(t) = state.get_transfer(&transfer_id) {
        let _ = app.emit("transfer-finished", &t);
    }
    Ok(())
}
