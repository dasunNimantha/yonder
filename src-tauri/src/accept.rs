use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use iroh::endpoint::{Connection, Endpoint, RecvStream, SendStream};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot;

use crate::state::{AppState, ApprovalDecision};
use crate::transfer::{FileMeta, ProgressEvent, ProgressThrottle, Transfer, TransferStatus};

/// Maximum size we'll allocate for the meta JSON header. Plenty for
/// thousands of files; protects against a buggy/malicious peer.
const MAX_META_BYTES: u32 = 1024 * 1024;

/// What the sender writes immediately after `open_bi`. Same shape as
/// the old multipart `meta` field; transports a session id plus the
/// list of files-to-be-streamed in order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadMeta {
    pub session: String,
    pub files: Vec<FileMeta>,
}

const ACCEPT: u8 = 1;
const REJECT: u8 = 0;
const OK: u8 = 0;
const ERR: u8 = 1;

/// Long-running task that drives the QUIC accept loop. Each accepted
/// connection is handed off to a `tokio::spawn` so multiple concurrent
/// transfers from different peers don't block each other.
pub async fn run_accept_loop(endpoint: Endpoint, state: AppState, app: AppHandle) {
    log::info!(
        "yonder accept loop started; endpoint id = {}",
        endpoint.id()
    );
    while let Some(incoming) = endpoint.accept().await {
        let connecting = match incoming.accept() {
            Ok(c) => c,
            Err(e) => {
                log::warn!("incoming.accept() failed: {e}");
                continue;
            }
        };
        let state = state.clone();
        let app = app.clone();
        tokio::spawn(async move {
            let conn = match connecting.await {
                Ok(c) => c,
                Err(e) => {
                    log::warn!("handshake failed: {e}");
                    return;
                }
            };
            if let Err(e) = handle_connection(conn, state, app).await {
                log::warn!("connection handler error: {e:#}");
            }
        });
    }
    log::info!("accept loop exiting");
}

async fn handle_connection(conn: Connection, state: AppState, app: AppHandle) -> Result<()> {
    let remote_id = conn.remote_id().to_string();
    log::info!("incoming QUIC connection from {remote_id}");

    let (mut send, mut recv) = conn.accept_bi().await.context("accept_bi")?;

    // ── 1. Read meta header (length-prefixed JSON).
    let meta = read_meta(&mut recv).await?;

    // ── 2. Resolve sender display name from mDNS state, else fall
    //       back to a short id so the receive prompt has something
    //       human-friendly to show.
    let peer = state.get_peer(&remote_id);
    let peer_name = peer
        .as_ref()
        .map(|p| p.name.clone())
        .unwrap_or_else(|| format!("Device {}", &remote_id[..remote_id.len().min(8)]));

    let transfer = Transfer::new_receive(
        meta.session.clone(),
        remote_id.clone(),
        peer_name.clone(),
        meta.files.clone(),
    );
    state.upsert_transfer(transfer.clone());
    let _ = app.emit("transfer-added", &transfer);

    // ── 3. Approval gate. Auto-accept skips the modal entirely.
    let auto_accept = state.settings().auto_accept;
    let decision = if auto_accept {
        ApprovalDecision::Accept
    } else {
        await_approval(&app, &state, &transfer).await
    };

    match decision {
        ApprovalDecision::Accept => {
            send.write_u8(ACCEPT).await.context("write accept byte")?;
        }
        ApprovalDecision::Reject => {
            let _ = send.write_u8(REJECT).await;
            let _ = send.finish();
            let _ = state.update_transfer(&transfer.id, |t| {
                t.status = TransferStatus::Rejected;
                t.finished_at = Some(Utc::now());
            });
            if let Some(t) = state.get_transfer(&transfer.id) {
                let _ = app.emit("transfer-finished", &t);
            }
            return Ok(());
        }
    }

    // ── 4. Mark active and spawn-throttle progress.
    let _ = state.update_transfer(&transfer.id, |t| {
        t.status = TransferStatus::Active;
    });
    if let Some(t) = state.get_transfer(&transfer.id) {
        let _ = app.emit("transfer-started", &t);
    }

    let download_dir = PathBuf::from(&state.settings().download_dir);
    if let Err(e) = tokio::fs::create_dir_all(&download_dir).await {
        let msg = format!("could not create download dir: {e}");
        return finish_failed(&mut send, &state, &app, &transfer.id, msg).await;
    }

    let throttle = Arc::new(ProgressThrottle::new(120));
    for file in &meta.files {
        if let Err(e) = receive_file(
            &mut recv,
            file,
            &download_dir,
            &throttle,
            transfer.total_bytes,
            &transfer.id,
            &state,
            &app,
        )
        .await
        {
            let msg = format!("recv {} failed: {e}", file.name);
            return finish_failed(&mut send, &state, &app, &transfer.id, msg).await;
        }
    }

    // ── 5. Mark complete + emit final progress.
    let final_bytes = throttle.snapshot();
    let _ = state.update_transfer(&transfer.id, |t| {
        t.bytes_done = final_bytes;
        t.status = TransferStatus::Completed;
        t.finished_at = Some(Utc::now());
    });
    if let Some(t) = state.get_transfer(&transfer.id) {
        let _ = app.emit(
            "transfer-progress",
            ProgressEvent {
                id: transfer.id.clone(),
                bytes_done: t.bytes_done,
                total_bytes: t.total_bytes,
                status: TransferStatus::Completed,
            },
        );
        let _ = app.emit("transfer-finished", &t);
    }

    let _ = send.write_u8(OK).await;
    let _ = send.finish();

    // The sender is the peer receiving our LAST application byte
    // (the completion ack above), so per iroh's graceful-close docs
    // the sender is responsible for closing the connection. We keep
    // *our* end alive until that happens (with a generous cap to
    // bound resource use against a misbehaving peer) so the OK byte
    // makes it across before our Connection handle drops on task
    // exit. Without this, the sender would routinely surface a
    // false-positive "read completion byte" error even though the
    // files arrived intact.
    let _ = tokio::time::timeout(Duration::from_secs(10), conn.closed()).await;

    // Fire-and-forget desktop notification so the user sees a
    // confirmation even if the window is hidden in the tray.
    let notify_app = app.clone();
    let title = "Transfer complete".to_string();
    let body = match meta.files.len() {
        1 => format!("Received {} from {}", meta.files[0].name, peer_name),
        n => format!("Received {n} files from {peer_name}"),
    };
    tauri::async_runtime::spawn(async move {
        let _ = notify_app
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show();
    });

    Ok(())
}

async fn read_meta(recv: &mut RecvStream) -> Result<UploadMeta> {
    let len = recv.read_u32_le().await.context("read meta len")?;
    if len == 0 || len > MAX_META_BYTES {
        return Err(anyhow!("invalid meta length: {len}"));
    }
    let mut buf = vec![0u8; len as usize];
    recv.read_exact(&mut buf).await.context("read meta body")?;
    let meta: UploadMeta = serde_json::from_slice(&buf).context("parse meta JSON")?;
    if meta.files.is_empty() {
        return Err(anyhow!("meta has no files"));
    }
    Ok(meta)
}

async fn await_approval(
    app: &AppHandle,
    state: &AppState,
    transfer: &Transfer,
) -> ApprovalDecision {
    let (tx, rx) = oneshot::channel();
    state.register_pending_approval(&transfer.id, tx);
    let _ = app.emit("transfer-awaiting-approval", transfer);

    // Surface the request as an OS-level notification so a user with
    // the window hidden in the tray still sees the prompt. We fire it
    // from a detached task so a slow desktop notification daemon
    // can't delay the IPC event.
    let notify_app = app.clone();
    let title = format!("{} wants to send files", transfer.peer_name);
    let body = match transfer.files.len() {
        1 => format!(
            "{} ({})",
            transfer.files[0].name,
            format_bytes(transfer.total_bytes)
        ),
        n => format!("{n} files ({})", format_bytes(transfer.total_bytes)),
    };
    tauri::async_runtime::spawn(async move {
        let _ = notify_app
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show();
    });

    rx.await.unwrap_or(ApprovalDecision::Reject)
}

fn format_bytes(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut i = 0;
    while value >= 1024.0 && i < units.len() - 1 {
        value /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} {}", bytes, units[i])
    } else {
        format!("{:.1} {}", value, units[i])
    }
}

#[allow(clippy::too_many_arguments)]
async fn receive_file(
    recv: &mut RecvStream,
    meta: &FileMeta,
    download_dir: &Path,
    throttle: &Arc<ProgressThrottle>,
    total_bytes: u64,
    transfer_id: &str,
    state: &AppState,
    app: &AppHandle,
) -> Result<()> {
    let safe_name = sanitize_filename::sanitize(&meta.name);
    let dest = unique_path(download_dir, &safe_name);
    let mut file = File::create(&dest)
        .await
        .with_context(|| format!("create {}", dest.display()))?;

    let mut remaining = meta.size;
    let mut buf = vec![0u8; 64 * 1024];
    while remaining > 0 {
        let want = remaining.min(buf.len() as u64) as usize;
        let n = recv
            .read(&mut buf[..want])
            .await
            .context("stream read")?
            .ok_or_else(|| anyhow!("peer closed stream early"))?;
        if n == 0 {
            return Err(anyhow!("peer closed stream early"));
        }
        file.write_all(&buf[..n]).await.context("disk write")?;
        remaining -= n as u64;

        if let Some(bytes_done) = throttle.add(n as u64) {
            let _ = state.update_transfer(transfer_id, |t| {
                t.bytes_done = bytes_done;
            });
            let _ = app.emit(
                "transfer-progress",
                ProgressEvent {
                    id: transfer_id.to_string(),
                    bytes_done,
                    total_bytes,
                    status: TransferStatus::Active,
                },
            );
        } else {
            let snap = throttle.snapshot();
            let _ = state.update_transfer(transfer_id, |t| {
                t.bytes_done = snap;
            });
        }
    }
    file.flush().await.context("flush")?;
    Ok(())
}

async fn finish_failed(
    send: &mut SendStream,
    state: &AppState,
    app: &AppHandle,
    transfer_id: &str,
    msg: String,
) -> Result<()> {
    log::warn!("transfer {transfer_id} failed: {msg}");
    let _ = send.write_u8(ERR).await;
    let _ = send.finish();
    let _ = state.update_transfer(transfer_id, |t| {
        t.status = TransferStatus::Failed;
        t.error = Some(msg);
        t.finished_at = Some(Utc::now());
    });
    if let Some(t) = state.get_transfer(transfer_id) {
        let _ = app.emit("transfer-finished", &t);
    }
    Ok(())
}

/// Find a path that doesn't collide, appending `(2)`, `(3)`, …
pub(crate) fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let mut candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let stem = Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string());
    let ext = Path::new(name)
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    let mut n: u32 = 2;
    loop {
        candidate = dir.join(format!("{stem} ({n}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
        if n > 9999 {
            return candidate;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::unique_path;

    #[test]
    fn unique_path_returns_input_when_no_collision() {
        let dir = tempfile::tempdir().unwrap();
        let p = unique_path(dir.path(), "fresh.txt");
        assert_eq!(p, dir.path().join("fresh.txt"));
    }

    #[test]
    fn unique_path_appends_numeric_suffix_for_collisions() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("file.txt"), b"a").unwrap();
        let p2 = unique_path(dir.path(), "file.txt");
        assert_eq!(p2, dir.path().join("file (2).txt"));

        std::fs::write(&p2, b"b").unwrap();
        let p3 = unique_path(dir.path(), "file.txt");
        assert_eq!(p3, dir.path().join("file (3).txt"));
    }

    #[test]
    fn unique_path_handles_files_without_extension() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("README"), b"a").unwrap();
        let p = unique_path(dir.path(), "README");
        assert_eq!(p, dir.path().join("README (2)"));
    }

    #[test]
    fn unique_path_preserves_dotfile_extensions() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("archive.tar.gz"), b"a").unwrap();
        let p = unique_path(dir.path(), "archive.tar.gz");
        assert_eq!(p, dir.path().join("archive.tar (2).gz"));
    }
}
