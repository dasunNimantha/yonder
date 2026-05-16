use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Send,
    Receive,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TransferStatus {
    Pending,
    AwaitingApproval,
    Active,
    Completed,
    Cancelled,
    Failed,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMeta {
    pub name: String,
    pub size: u64,
    #[serde(default)]
    pub mime: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transfer {
    pub id: String,
    pub direction: Direction,
    pub peer_id: String,
    pub peer_name: String,
    pub files: Vec<FileMeta>,
    pub total_bytes: u64,
    pub bytes_done: u64,
    pub status: TransferStatus,
    #[serde(default)]
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Utc>>,
}

impl Transfer {
    pub fn new_send(peer_id: String, peer_name: String, files: Vec<FileMeta>) -> Self {
        let total_bytes = files.iter().map(|f| f.size).sum();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            direction: Direction::Send,
            peer_id,
            peer_name,
            files,
            total_bytes,
            bytes_done: 0,
            status: TransferStatus::Pending,
            error: None,
            started_at: Utc::now(),
            finished_at: None,
        }
    }

    pub fn new_receive(
        id: String,
        peer_id: String,
        peer_name: String,
        files: Vec<FileMeta>,
    ) -> Self {
        let total_bytes = files.iter().map(|f| f.size).sum();
        Self {
            id,
            direction: Direction::Receive,
            peer_id,
            peer_name,
            files,
            total_bytes,
            bytes_done: 0,
            status: TransferStatus::AwaitingApproval,
            error: None,
            started_at: Utc::now(),
            finished_at: None,
        }
    }
}

/// Lightweight progress event payload. Emitted from server / client
/// streams throttled by [`ProgressEmitter`] to avoid swamping the
/// frontend with a per-chunk update.
#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    pub id: String,
    pub bytes_done: u64,
    pub total_bytes: u64,
    pub status: TransferStatus,
}

/// Coalesces frequent byte-count updates into ~10 events/sec per
/// transfer. Each call to `add` returns the cumulative bytes to emit if
/// enough time has elapsed since the last emission.
pub struct ProgressThrottle {
    bytes: AtomicU64,
    last_emit_ms: AtomicU64,
    interval_ms: u64,
}

impl ProgressThrottle {
    pub fn new(interval_ms: u64) -> Self {
        Self {
            bytes: AtomicU64::new(0),
            last_emit_ms: AtomicU64::new(0),
            interval_ms,
        }
    }

    pub fn add(&self, delta: u64) -> Option<u64> {
        let new_bytes = self.bytes.fetch_add(delta, Ordering::Relaxed) + delta;
        let now = now_ms();
        let last = self.last_emit_ms.load(Ordering::Relaxed);
        if now.saturating_sub(last) >= self.interval_ms {
            // Best-effort CAS: if another thread emitted first, skip.
            if self
                .last_emit_ms
                .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return Some(new_bytes);
            }
        }
        None
    }

    pub fn snapshot(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
