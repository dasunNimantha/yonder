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

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(name: &str, size: u64) -> FileMeta {
        FileMeta {
            name: name.into(),
            size,
            mime: None,
        }
    }

    #[test]
    fn new_send_sums_total_bytes_and_assigns_unique_id() {
        let t1 = Transfer::new_send(
            "p1".into(),
            "Bob".into(),
            vec![meta("a.txt", 100), meta("b.txt", 250)],
        );
        let t2 = Transfer::new_send("p1".into(), "Bob".into(), vec![meta("a.txt", 100)]);

        assert_eq!(t1.total_bytes, 350);
        assert_eq!(t1.bytes_done, 0);
        assert_eq!(t1.direction, Direction::Send);
        assert_eq!(t1.status, TransferStatus::Pending);
        assert_ne!(t1.id, t2.id, "transfer ids should not collide");
    }

    #[test]
    fn new_receive_starts_in_awaiting_approval() {
        let t = Transfer::new_receive(
            "session-1".into(),
            "p2".into(),
            "Alice".into(),
            vec![meta("doc.pdf", 4096)],
        );
        assert_eq!(t.id, "session-1");
        assert_eq!(t.direction, Direction::Receive);
        assert_eq!(t.status, TransferStatus::AwaitingApproval);
        assert_eq!(t.total_bytes, 4096);
    }

    #[test]
    fn progress_throttle_emits_first_then_pauses() {
        // With a 1-second interval, the first add of any size should
        // emit (last_emit_ms starts at 0, so the diff is huge), and
        // immediate follow-ups must NOT emit.
        let t = ProgressThrottle::new(1000);
        let first = t.add(100);
        assert_eq!(first, Some(100));
        let second = t.add(50);
        assert_eq!(second, None, "rapid follow-up should be throttled");
        assert_eq!(t.snapshot(), 150);
    }

    #[test]
    fn progress_throttle_emits_again_after_interval() {
        // 0ms interval means every call should emit.
        let t = ProgressThrottle::new(0);
        for _ in 0..5 {
            assert!(t.add(10).is_some());
        }
        assert_eq!(t.snapshot(), 50);
    }

    #[test]
    fn progress_throttle_accumulates_bytes_across_skipped_emits() {
        let t = ProgressThrottle::new(100_000); // effectively never emits past first
        let _ = t.add(1);
        for _ in 0..9 {
            let _ = t.add(1);
        }
        assert_eq!(t.snapshot(), 10);
    }
}
