use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use iroh::endpoint::{Endpoint, SendStream};
use iroh::EndpointAddr;
use tauri::{AppHandle, Emitter};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::accept::UploadMeta;
use crate::identity;
use crate::net::ALPN;
use crate::state::{AppState, Peer};
use crate::transfer::{FileMeta, ProgressEvent, ProgressThrottle, Transfer, TransferStatus};

/// Spawn an async task that connects to `peer` over QUIC and streams
/// every `path` in turn. Returns the new transfer id immediately.
pub fn spawn_send(
    handle: AppHandle,
    state: AppState,
    endpoint: Endpoint,
    peer: Peer,
    paths: Vec<PathBuf>,
) -> Result<String> {
    let metas = paths
        .iter()
        .map(|p| {
            let md =
                std::fs::metadata(p).map_err(|e| anyhow!("could not stat {}: {e}", p.display()))?;
            let name = p
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unnamed".to_string());
            Ok::<_, anyhow::Error>(FileMeta {
                name,
                size: md.len(),
                mime: None,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let transfer = Transfer::new_send(peer.id.clone(), peer.name.clone(), metas.clone());
    let transfer_id = transfer.id.clone();
    state.upsert_transfer(transfer.clone());
    let _ = handle.emit("transfer-added", &transfer);

    let session = transfer_id.clone();
    let handle_for_task = handle.clone();
    let state_for_task = state.clone();

    // `spawn_send` is invoked from Tauri's *sync* command handler,
    // which runs on Tauri's blocking-thread pool — there's no tokio
    // runtime in that thread's task-local context, so `tokio::spawn`
    // panics with "there is no reactor running". `tauri::async_runtime::spawn`
    // is runtime-agnostic and dispatches onto Tauri's managed tokio
    // runtime no matter where it's called from.
    tauri::async_runtime::spawn(async move {
        if let Err(e) = do_send(
            handle_for_task.clone(),
            state_for_task.clone(),
            endpoint,
            peer,
            session.clone(),
            paths,
            metas,
        )
        .await
        {
            log::error!("send failed: {e:#}");
            let _ = state_for_task.update_transfer(&session, |t| {
                t.status = TransferStatus::Failed;
                t.error = Some(e.to_string());
                t.finished_at = Some(Utc::now());
            });
            if let Some(t) = state_for_task.get_transfer(&session) {
                let _ = handle_for_task.emit("transfer-finished", &t);
            }
        }
    });

    Ok(transfer_id)
}

#[allow(clippy::too_many_arguments)]
async fn do_send(
    handle: AppHandle,
    state: AppState,
    endpoint: Endpoint,
    peer: Peer,
    session: String,
    paths: Vec<PathBuf>,
    metas: Vec<FileMeta>,
) -> Result<()> {
    let endpoint_id = identity::parse_endpoint_id(&peer.id)?;
    let addr = EndpointAddr::new(endpoint_id);

    log::info!("connecting to {} ({})", peer.name, peer.id);
    let conn = endpoint.connect(addr, ALPN).await.context("QUIC connect")?;
    let (mut send, mut recv) = conn.open_bi().await.context("open_bi")?;

    // ── 1. Mark active and send the meta header.
    let _ = state.update_transfer(&session, |t| {
        t.status = TransferStatus::Active;
    });
    if let Some(t) = state.get_transfer(&session) {
        let _ = handle.emit("transfer-started", &t);
    }

    let upload_meta = UploadMeta {
        session: session.clone(),
        files: metas.clone(),
    };
    write_meta(&mut send, &upload_meta).await?;

    // ── 2. Read accept/reject byte from peer.
    let decision = recv.read_u8().await.context("read accept byte")?;
    if decision != 1 {
        return Err(anyhow!("peer rejected transfer"));
    }

    // ── 3. Stream each file in order.
    let total_bytes: u64 = metas.iter().map(|m| m.size).sum();
    let throttle = Arc::new(ProgressThrottle::new(120));

    for (path, file_meta) in paths.iter().zip(metas.iter()) {
        send_file(
            &mut send,
            path,
            file_meta,
            &throttle,
            total_bytes,
            &session,
            &state,
            &handle,
        )
        .await?;
    }

    // Flush + finish before reading the completion ack so the peer
    // sees the end of the last file.
    send.finish().context("finish stream")?;

    // ── 4. Read the completion byte and any error message.
    let ok = recv.read_u8().await.context("read completion byte")?;
    if ok != 0 {
        return Err(anyhow!("peer reported transfer error"));
    }

    let final_bytes = throttle.snapshot();
    let _ = state.update_transfer(&session, |t| {
        t.bytes_done = if final_bytes > 0 {
            final_bytes
        } else {
            total_bytes
        };
        t.status = TransferStatus::Completed;
        t.finished_at = Some(Utc::now());
    });
    if let Some(t) = state.get_transfer(&session) {
        let _ = handle.emit(
            "transfer-progress",
            ProgressEvent {
                id: session.clone(),
                bytes_done: t.bytes_done,
                total_bytes: t.total_bytes,
                status: TransferStatus::Completed,
            },
        );
        let _ = handle.emit("transfer-finished", &t);
    }
    Ok(())
}

async fn write_meta(send: &mut SendStream, meta: &UploadMeta) -> Result<()> {
    let bytes = serde_json::to_vec(meta).context("serialize meta")?;
    let len: u32 = bytes
        .len()
        .try_into()
        .map_err(|_| anyhow!("meta too large"))?;
    send.write_u32_le(len).await.context("write meta len")?;
    send.write_all(&bytes).await.context("write meta body")?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn send_file(
    send: &mut SendStream,
    path: &std::path::Path,
    meta: &FileMeta,
    throttle: &Arc<ProgressThrottle>,
    total_bytes: u64,
    session: &str,
    state: &AppState,
    app: &AppHandle,
) -> Result<()> {
    let mut file = File::open(path)
        .await
        .with_context(|| format!("open {}", path.display()))?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut remaining = meta.size;
    while remaining > 0 {
        let want = remaining.min(buf.len() as u64) as usize;
        let n = file.read(&mut buf[..want]).await.context("file read")?;
        if n == 0 {
            return Err(anyhow!(
                "{} shrank during transfer (expected {} more bytes)",
                path.display(),
                remaining
            ));
        }
        send.write_all(&buf[..n]).await.context("stream write")?;
        remaining -= n as u64;

        if let Some(bytes_done) = throttle.add(n as u64) {
            let _ = state.update_transfer(session, |t| {
                t.bytes_done = bytes_done;
            });
            let _ = app.emit(
                "transfer-progress",
                ProgressEvent {
                    id: session.to_string(),
                    bytes_done,
                    total_bytes,
                    status: TransferStatus::Active,
                },
            );
        } else {
            let snap = throttle.snapshot();
            let _ = state.update_transfer(session, |t| {
                t.bytes_done = snap;
            });
        }
    }
    Ok(())
}
