use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use chrono::Utc;
use futures_util::TryStreamExt;
use reqwest::multipart::{Form, Part};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::state::{AppState, Peer};
use crate::transfer::{
    FileMeta, ProgressEvent, ProgressThrottle, Transfer, TransferStatus,
};

#[derive(Debug, Clone, Serialize)]
struct UploadMetaBody {
    files: Vec<FileMeta>,
}

/// Probe a peer's `/info` endpoint to sanity-check it's still up before
/// streaming a large transfer.
pub async fn probe(peer: &Peer) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let url = format!("http://{}:{}/info", peer.host, peer.port);
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(anyhow!("peer /info returned {}", resp.status()));
    }
    Ok(())
}

/// Spawn an async task that sends all of `paths` to `peer` and emits
/// progress events. Returns immediately with the created Transfer id.
pub fn spawn_send(
    handle: AppHandle,
    state: AppState,
    peer: Peer,
    paths: Vec<PathBuf>,
) -> Result<String> {
    let metas = paths
        .iter()
        .map(|p| {
            let md = std::fs::metadata(p)
                .map_err(|e| anyhow!("could not stat {}: {e}", p.display()))?;
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

    let handle_for_task = handle.clone();
    let state_for_task = state.clone();
    let session = transfer_id.clone();
    let identity = state.identity();

    tokio::spawn(async move {
        if let Err(e) = do_send(
            handle_for_task.clone(),
            state_for_task.clone(),
            peer.clone(),
            session.clone(),
            paths.clone(),
            metas.clone(),
            identity.id.clone(),
            identity.name.clone(),
        )
        .await
        {
            log::error!("send failed: {e}");
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
    peer: Peer,
    session: String,
    paths: Vec<PathBuf>,
    metas: Vec<FileMeta>,
    our_id: String,
    our_name: String,
) -> Result<()> {
    probe(&peer).await?;

    let _ = state.update_transfer(&session, |t| {
        t.status = TransferStatus::Active;
    });
    if let Some(t) = state.get_transfer(&session) {
        let _ = handle.emit("transfer-started", &t);
    }

    let total_bytes: u64 = metas.iter().map(|m| m.size).sum();
    let throttle = Arc::new(ProgressThrottle::new(120));

    let meta_body = UploadMetaBody {
        files: metas.clone(),
    };
    let meta_json = serde_json::to_string(&meta_body)?;

    let mut form = Form::new().part(
        "meta",
        Part::text(meta_json).mime_str("application/json")?,
    );

    for (path, meta) in paths.iter().zip(metas.iter()) {
        let file = File::open(path)
            .await
            .map_err(|e| anyhow!("could not open {}: {e}", path.display()))?;

        let reader_stream = ReaderStream::new(file);
        let progress_throttle = Arc::clone(&throttle);
        let progress_handle = handle.clone();
        let progress_state = state.clone();
        let progress_session = session.clone();

        let counting = reader_stream.inspect_ok(move |chunk| {
            if let Some(bytes_done) = progress_throttle.add(chunk.len() as u64) {
                let _ = progress_state.update_transfer(&progress_session, |t| {
                    t.bytes_done = bytes_done;
                });
                let _ = progress_handle.emit(
                    "transfer-progress",
                    ProgressEvent {
                        id: progress_session.clone(),
                        bytes_done,
                        total_bytes,
                        status: TransferStatus::Active,
                    },
                );
            }
        });

        let body = reqwest::Body::wrap_stream(counting);
        let mime = guess_mime(path);
        let part = Part::stream_with_length(body, meta.size)
            .file_name(meta.name.clone())
            .mime_str(&mime)?;
        form = form.part("file", part);
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60 * 60 * 12))
        .build()?;

    let url = format!(
        "http://{}:{}/upload?session={}&sender={}&sender_name={}",
        peer.host,
        peer.port,
        urlencode(&session),
        urlencode(&our_id),
        urlencode(&our_name),
    );

    let resp = client
        .post(&url)
        .multipart(form)
        .send()
        .await
        .map_err(|e| anyhow!("send failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("peer rejected upload ({status}): {body}"));
    }

    let final_bytes = throttle.snapshot();
    let _ = state.update_transfer(&session, |t| {
        t.bytes_done = if final_bytes > 0 { final_bytes } else { total_bytes };
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

fn urlencode(s: &str) -> String {
    use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
    utf8_percent_encode(s, NON_ALPHANUMERIC).to_string()
}

fn guess_mime(path: &std::path::Path) -> String {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "txt" | "md" | "log" => "text/plain",
        "json" => "application/json",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "application/javascript",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
    .to_string()
}
