use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use axum::{
    extract::{Multipart, Query, State as AxumState},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio::sync::oneshot;

use crate::state::{AppState, ApprovalDecision};
use crate::transfer::{FileMeta, ProgressEvent, ProgressThrottle, Transfer, TransferStatus};

#[derive(Clone)]
struct ServerCtx {
    handle: AppHandle,
    state: AppState,
}

#[derive(Debug, Clone, Serialize)]
struct InfoResponse {
    id: String,
    name: String,
    os: String,
    version: String,
}

#[derive(Debug, Deserialize)]
struct UploadParams {
    /// Caller-supplied transfer id.
    session: String,
    /// Sender identity, must match an mDNS-discovered peer.
    sender: String,
    /// Sender display name (used for the receive prompt when the peer
    /// hasn't been seen yet via mDNS, which can happen on a fast send).
    #[serde(default)]
    sender_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UploadMeta {
    files: Vec<FileMeta>,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

fn err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ErrorBody>) {
    (status, Json(ErrorBody { error: msg.into() }))
}

/// Spawn the HTTP receive server on `0.0.0.0:port`. Returns a handle
/// to the bound socket address (useful for the discovery announcement).
pub async fn spawn(handle: AppHandle, state: AppState, port: u16) -> Result<SocketAddr> {
    let ctx = ServerCtx {
        handle: handle.clone(),
        state,
    };

    let app = Router::new()
        .route("/info", get(get_info))
        .route("/upload", post(post_upload))
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_methods(tower_http::cors::Any)
                .allow_origin(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any),
        )
        // Body size cap: 50 GB per upload session. Bumping this is safe;
        // we stream to disk so memory use is bounded by the read buffer
        // size, not the body size.
        .layer(tower_http::limit::RequestBodyLimitLayer::new(
            50 * 1024 * 1024 * 1024,
        ))
        .with_state(Arc::new(ctx));

    let addr: SocketAddr = format!("0.0.0.0:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| anyhow!("could not bind {addr}: {e}"))?;
    let bound = listener.local_addr()?;
    log::info!("yonder server listening on {bound}");

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            log::error!("yonder server crashed: {e}");
        }
    });

    Ok(bound)
}

async fn get_info(AxumState(ctx): AxumState<Arc<ServerCtx>>) -> impl IntoResponse {
    let id = ctx.state.identity();
    Json(InfoResponse {
        id: id.id,
        name: id.name,
        os: id.os,
        version: id.version,
    })
}

#[axum::debug_handler(state = Arc<ServerCtx>)]
async fn post_upload(
    AxumState(ctx): AxumState<Arc<ServerCtx>>,
    Query(params): Query<UploadParams>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorBody>)> {
    // ── 1. Read the metadata part. We REQUIRE this to be the first
    //       field so we can show the receive prompt before allocating
    //       disk space for the actual files.
    let meta_field = multipart
        .next_field()
        .await
        .map_err(|e| err(StatusCode::BAD_REQUEST, format!("multipart error: {e}")))?
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "missing meta part"))?;

    if meta_field.name() != Some("meta") {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "first multipart field must be 'meta'",
        ));
    }
    let meta_bytes = meta_field
        .bytes()
        .await
        .map_err(|e| err(StatusCode::BAD_REQUEST, format!("meta read error: {e}")))?;
    let meta: UploadMeta = serde_json::from_slice(&meta_bytes)
        .map_err(|e| err(StatusCode::BAD_REQUEST, format!("invalid meta JSON: {e}")))?;

    if meta.files.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "no files in meta"));
    }

    // ── 2. Resolve sender. We look up the peer by id from mDNS state
    //       so we have a "verified" name; fall back to sender_name from
    //       the query if the peer hasn't been seen yet.
    let peer = ctx.state.get_peer(&params.sender);
    let peer_name = peer
        .as_ref()
        .map(|p| p.name.clone())
        .or(params.sender_name.clone())
        .unwrap_or_else(|| "Unknown device".to_string());

    let transfer = Transfer::new_receive(
        params.session.clone(),
        params.sender.clone(),
        peer_name.clone(),
        meta.files.clone(),
    );
    ctx.state.upsert_transfer(transfer.clone());

    let _ = ctx.handle.emit("transfer-added", &transfer);

    // ── 3. Approval gate: if auto_accept is off, await user decision.
    let auto_accept = ctx.state.settings().auto_accept;
    if !auto_accept {
        let (tx, rx) = oneshot::channel();
        ctx.state.register_pending_approval(&params.session, tx);
        let _ = ctx.handle.emit("transfer-awaiting-approval", &transfer);
        match rx.await {
            Ok(ApprovalDecision::Accept) => {}
            Ok(ApprovalDecision::Reject) | Err(_) => {
                let _ = ctx.state.update_transfer(&params.session, |t| {
                    t.status = TransferStatus::Rejected;
                    t.finished_at = Some(Utc::now());
                });
                if let Some(t) = ctx.state.get_transfer(&params.session) {
                    let _ = ctx.handle.emit("transfer-finished", &t);
                }
                return Err(err(StatusCode::FORBIDDEN, "receiver rejected transfer"));
            }
        }
    }

    let _ = ctx.state.update_transfer(&params.session, |t| {
        t.status = TransferStatus::Active;
    });
    if let Some(t) = ctx.state.get_transfer(&params.session) {
        let _ = ctx.handle.emit("transfer-started", &t);
    }

    // ── 4. Stream each remaining part to disk.
    let download_dir = PathBuf::from(&ctx.state.settings().download_dir);
    if let Err(e) = tokio::fs::create_dir_all(&download_dir).await {
        return Err(err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("could not create download dir: {e}"),
        ));
    }

    let throttle = ProgressThrottle::new(120);
    let mut saved_paths: Vec<String> = Vec::new();

    while let Some(mut field) = multipart.next_field().await.map_err(|e| {
        err(
            StatusCode::BAD_REQUEST,
            format!("multipart read error: {e}"),
        )
    })? {
        // Per-file name; ignore parts without a filename.
        let raw_name = match field.file_name().map(|s| s.to_string()) {
            Some(n) => n,
            None => continue,
        };
        let safe_name = sanitize_filename::sanitize(&raw_name);
        let dest = unique_path(&download_dir, &safe_name);

        let mut file = match tokio::fs::File::create(&dest).await {
            Ok(f) => f,
            Err(e) => {
                return Err(fail_transfer(
                    &ctx,
                    &params.session,
                    format!("write failed for {raw_name}: {e}"),
                ))
            }
        };

        loop {
            match field.chunk().await {
                Ok(Some(chunk)) => {
                    if let Err(e) = file.write_all(&chunk).await {
                        return Err(fail_transfer(
                            &ctx,
                            &params.session,
                            format!("disk write failed: {e}"),
                        ));
                    }
                    if let Some(bytes_done) = throttle.add(chunk.len() as u64) {
                        let _ = ctx.state.update_transfer(&params.session, |t| {
                            t.bytes_done = bytes_done;
                        });
                        let _ = ctx.handle.emit(
                            "transfer-progress",
                            ProgressEvent {
                                id: params.session.clone(),
                                bytes_done,
                                total_bytes: transfer.total_bytes,
                                status: TransferStatus::Active,
                            },
                        );
                    } else {
                        // Still update internal state cheaply so
                        // list_transfers shows current progress.
                        let snap = throttle.snapshot();
                        let _ = ctx.state.update_transfer(&params.session, |t| {
                            t.bytes_done = snap;
                        });
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    return Err(fail_transfer(
                        &ctx,
                        &params.session,
                        format!("network read failed: {e}"),
                    ))
                }
            }
        }

        if let Err(e) = file.flush().await {
            return Err(fail_transfer(
                &ctx,
                &params.session,
                format!("flush failed: {e}"),
            ));
        }
        saved_paths.push(dest.to_string_lossy().to_string());
    }

    // ── 5. Mark complete + emit final progress.
    let final_bytes = throttle.snapshot();
    let _ = ctx.state.update_transfer(&params.session, |t| {
        t.bytes_done = final_bytes;
        t.status = TransferStatus::Completed;
        t.finished_at = Some(Utc::now());
    });
    if let Some(t) = ctx.state.get_transfer(&params.session) {
        let _ = ctx.handle.emit(
            "transfer-progress",
            ProgressEvent {
                id: params.session.clone(),
                bytes_done: t.bytes_done,
                total_bytes: t.total_bytes,
                status: TransferStatus::Completed,
            },
        );
        let _ = ctx.handle.emit("transfer-finished", &t);
    }

    Ok(Json(serde_json::json!({
        "ok": true,
        "files": saved_paths,
    })))
}

fn fail_transfer(ctx: &ServerCtx, session: &str, msg: String) -> (StatusCode, Json<ErrorBody>) {
    let _ = ctx.state.update_transfer(session, |t| {
        t.status = TransferStatus::Failed;
        t.error = Some(msg.clone());
        t.finished_at = Some(Utc::now());
    });
    if let Some(t) = ctx.state.get_transfer(session) {
        let _ = ctx.handle.emit("transfer-finished", &t);
    }
    err(StatusCode::INTERNAL_SERVER_ERROR, msg)
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
        // Path::extension only sees the last segment, so we get
        // "archive.tar (2).gz". This matches user-visible behaviour
        // of most file managers.
        let p = unique_path(dir.path(), "archive.tar.gz");
        assert_eq!(p, dir.path().join("archive.tar (2).gz"));
    }
}
