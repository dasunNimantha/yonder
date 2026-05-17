use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::oneshot;

use crate::config::Settings;
use crate::identity::Identity;
use crate::transfer::Transfer;

/// Decision returned by the receive prompt to the QUIC accept handler.
#[derive(Debug, Clone, Copy)]
pub enum ApprovalDecision {
    Accept,
    Reject,
}

/// A peer discovered via iroh's mDNS-based address lookup. The `id`
/// is the peer's [`iroh::EndpointId`] rendered as a string and is the
/// only address you need to dial them — iroh resolves it back to a
/// `EndpointAddr` internally using the discovered direct addresses.
#[derive(Debug, Clone, Serialize)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub os: String,
    pub version: String,
    /// Direct UDP addresses learned via mDNS. Informational only —
    /// dialing happens by `id`. Populated for tooltips and debugging.
    pub addresses: Vec<String>,
}

/// In-memory app state. Cheap to clone because everything is Arc<Mutex>.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    settings: Mutex<Settings>,
    identity: Mutex<Identity>,
    peers: Mutex<HashMap<String, Peer>>,
    transfers: Mutex<HashMap<String, Transfer>>,
    /// Pending oneshot senders keyed by transfer id; the QUIC accept
    /// handler awaits these to know whether the user accepted.
    pending_approvals: Mutex<HashMap<String, oneshot::Sender<ApprovalDecision>>>,
    /// Per-transfer cancellation flags. The send/receive loops check
    /// this between chunks; flipping it via `signal_cancel` causes
    /// the next chunk read/write to bail with a "cancelled" error.
    cancel_flags: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl AppState {
    pub fn new(settings: Settings, identity: Identity) -> Self {
        Self {
            inner: Arc::new(Inner {
                settings: Mutex::new(settings),
                identity: Mutex::new(identity),
                peers: Mutex::new(HashMap::new()),
                transfers: Mutex::new(HashMap::new()),
                pending_approvals: Mutex::new(HashMap::new()),
                cancel_flags: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn identity(&self) -> Identity {
        self.inner.identity.lock().unwrap().clone()
    }

    pub fn set_identity(&self, identity: Identity) {
        *self.inner.identity.lock().unwrap() = identity;
    }

    pub fn settings(&self) -> Settings {
        self.inner.settings.lock().unwrap().clone()
    }

    pub fn set_settings(&self, settings: Settings) {
        *self.inner.settings.lock().unwrap() = settings;
    }

    pub fn list_peers(&self) -> Vec<Peer> {
        self.inner.peers.lock().unwrap().values().cloned().collect()
    }

    pub fn get_peer(&self, id: &str) -> Option<Peer> {
        self.inner.peers.lock().unwrap().get(id).cloned()
    }

    /// Returns true if the peer was newly inserted (caller emits
    /// `peer-added`); false if it was an in-place update (caller may
    /// emit `peer-updated`).
    pub fn insert_peer(&self, peer: Peer) -> bool {
        let mut guard = self.inner.peers.lock().unwrap();
        let key = peer.id.clone();
        let existed = guard.contains_key(&key);
        guard.insert(key, peer);
        !existed
    }

    pub fn remove_peer(&self, id: &str) -> Option<Peer> {
        self.inner.peers.lock().unwrap().remove(id)
    }

    pub fn list_transfers(&self) -> Vec<Transfer> {
        self.inner
            .transfers
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect()
    }

    pub fn upsert_transfer(&self, t: Transfer) {
        self.inner.transfers.lock().unwrap().insert(t.id.clone(), t);
    }

    pub fn get_transfer(&self, id: &str) -> Option<Transfer> {
        self.inner.transfers.lock().unwrap().get(id).cloned()
    }

    /// Mutate a transfer in-place. Returns the new clone if it existed.
    pub fn update_transfer<F>(&self, id: &str, f: F) -> Option<Transfer>
    where
        F: FnOnce(&mut Transfer),
    {
        let mut guard = self.inner.transfers.lock().unwrap();
        if let Some(t) = guard.get_mut(id) {
            f(t);
            return Some(t.clone());
        }
        None
    }

    pub fn register_pending_approval(
        &self,
        transfer_id: &str,
        tx: oneshot::Sender<ApprovalDecision>,
    ) {
        self.inner
            .pending_approvals
            .lock()
            .unwrap()
            .insert(transfer_id.to_string(), tx);
    }

    pub fn take_pending_approval(
        &self,
        transfer_id: &str,
    ) -> Option<oneshot::Sender<ApprovalDecision>> {
        self.inner
            .pending_approvals
            .lock()
            .unwrap()
            .remove(transfer_id)
    }

    /// Allocate and register a cancellation flag for a new transfer.
    /// The streaming loops clone this Arc and poll it between chunks.
    pub fn register_cancel_flag(&self, transfer_id: &str) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        self.inner
            .cancel_flags
            .lock()
            .unwrap()
            .insert(transfer_id.to_string(), Arc::clone(&flag));
        flag
    }

    /// Trip the cancellation flag for a transfer if one is registered.
    /// Returns true if a flag existed (i.e. there was an in-flight
    /// transfer to cancel).
    pub fn signal_cancel(&self, transfer_id: &str) -> bool {
        if let Some(flag) = self.inner.cancel_flags.lock().unwrap().get(transfer_id) {
            flag.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Drop the cancel flag after a transfer reaches a terminal state
    /// so we don't leak handles forever.
    pub fn forget_cancel_flag(&self, transfer_id: &str) {
        self.inner.cancel_flags.lock().unwrap().remove(transfer_id);
    }
}
