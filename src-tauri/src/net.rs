use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use iroh::address_lookup::UserData;
use iroh::endpoint::presets;
use iroh::{Endpoint, SecretKey};
use iroh_mdns_address_lookup::{DiscoveryEvent, MdnsAddressLookup};
use n0_future::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::task::JoinHandle;

use crate::state::{AppState, Peer};

/// Grace period before a `peer-removed` event is actually emitted to
/// the frontend after iroh's mDNS reports the peer as expired. mDNS
/// announcements get dropped on contended Wi-Fi all the time, and
/// iroh will fire `Expired` followed by `Discovered` seconds later
/// for the same node — without this debounce the peer card flickers
/// out and back in. If the peer doesn't reappear within the window
/// we then emit `peer-removed` normally.
const PEER_REMOVAL_GRACE: Duration = Duration::from_secs(4);

/// ALPN identifier for the file-transfer protocol. Bumping the suffix
/// (`yonder/1`, …) lets us evolve the protocol without breaking older
/// clients on the same network.
pub const ALPN: &[u8] = b"yonder/0";

/// Compact JSON shape carried in the mDNS UserData TXT-equivalent so
/// other peers learn this device's display name and OS without us
/// having to round-trip a connection just to ask.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PeerUserData {
    n: String, // display name
    o: String, // os tag
    v: String, // version
}

#[derive(Debug, Clone, Serialize)]
struct PeerRemoved {
    id: String,
}

/// Build an iroh Endpoint with relay disabled and mDNS-only discovery.
/// Returns the Endpoint plus the bound MdnsAddressLookup whose
/// `subscribe()` is the canonical source of peer events.
pub async fn build_endpoint(
    secret_key: SecretKey,
    user_data: PeerUserDataIn,
) -> Result<(Endpoint, MdnsAddressLookup)> {
    let user_data = encode_user_data(&user_data)?;

    // `presets::N0DisableRelay` is iroh's "n0 defaults but with relay
    // disabled" preset. Combined with adding only `MdnsAddressLookup`
    // below this guarantees zero traffic ever leaves the LAN.
    let endpoint = Endpoint::builder(presets::N0DisableRelay)
        .secret_key(secret_key)
        .alpns(vec![ALPN.to_vec()])
        .user_data_for_address_lookup(user_data)
        .bind()
        .await
        .context("could not bind iroh endpoint")?;

    let mdns = MdnsAddressLookup::builder()
        .build(endpoint.id())
        .map_err(|e| anyhow::anyhow!("could not build mDNS lookup: {e}"))?;

    endpoint
        .address_lookup()
        .map_err(|e| anyhow::anyhow!("address-lookup unavailable: {e}"))?
        .add(mdns.clone());

    Ok((endpoint, mdns))
}

/// Inputs needed to build the broadcast UserData payload.
pub struct PeerUserDataIn {
    pub name: String,
    pub os: String,
    pub version: String,
}

fn encode_user_data(d: &PeerUserDataIn) -> Result<UserData> {
    let body = PeerUserData {
        n: d.name.clone(),
        o: d.os.clone(),
        v: d.version.clone(),
    };
    let json = serde_json::to_string(&body)?;
    UserData::try_from(json).map_err(|e| anyhow::anyhow!("user data too long: {e}"))
}

/// Update the broadcast UserData on the live endpoint (e.g. when the
/// user renames their device).
pub fn republish_user_data(endpoint: &Endpoint, d: &PeerUserDataIn) -> Result<()> {
    let user_data = encode_user_data(d)?;
    endpoint.set_user_data_for_address_lookup(Some(user_data));
    Ok(())
}

/// Long-running task: subscribe to mDNS discovery events and mirror
/// them into [`AppState`] / Tauri events for the frontend.
///
/// Returns when the discovery stream closes (which only happens when
/// the endpoint is shut down).
pub async fn run_discovery_loop(handle: AppHandle, state: AppState, mdns: MdnsAddressLookup) {
    let our_id = state.identity().id;
    let mut events = mdns.subscribe().await;

    // Pending peer-removal tasks keyed by endpoint id. We schedule a
    // deferred removal on `Expired`; if a `Discovered` for the same
    // id arrives before the timer fires, we abort the task so the
    // peer never appears to leave from the UI's perspective.
    let pending_removals: Arc<Mutex<HashMap<String, JoinHandle<()>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    while let Some(event) = events.next().await {
        match event {
            DiscoveryEvent::Discovered { endpoint_info, .. } => {
                let id = endpoint_info.endpoint_id.to_string();
                if id == our_id {
                    continue; // We don't want to see ourselves
                }
                // Cancel any pending removal for this peer — they're
                // back before we told the UI they left.
                if let Some(handle) = pending_removals.lock().unwrap().remove(&id) {
                    handle.abort();
                    log::debug!("peer {id} reappeared during grace period; removal cancelled");
                }
                let user_data = endpoint_info
                    .user_data()
                    .and_then(|u| serde_json::from_str::<PeerUserData>(u.as_ref()).ok());
                let (name, os, version) = match user_data {
                    Some(u) => (u.n, u.o, u.v),
                    None => (
                        "Unknown device".to_string(),
                        "unknown".to_string(),
                        String::new(),
                    ),
                };
                let addresses: Vec<String> =
                    endpoint_info.ip_addrs().map(|a| a.to_string()).collect();

                let peer = Peer {
                    id: id.clone(),
                    name,
                    os,
                    version,
                    addresses,
                };
                let inserted = state.insert_peer(peer.clone());
                if inserted {
                    log::info!("peer discovered: {} ({})", peer.name, peer.id);
                    let _ = handle.emit("peer-added", &peer);
                } else {
                    let _ = handle.emit("peer-updated", &peer);
                }
            }
            DiscoveryEvent::Expired { endpoint_id } => {
                let id = endpoint_id.to_string();
                if id == our_id {
                    continue;
                }
                // Defer the actual removal. mDNS announcements get
                // dropped on noisy Wi-Fi; if this is just a missed
                // beat the peer will be back well within the grace
                // period and the Discovered branch above will cancel
                // this task.
                schedule_peer_removal(
                    handle.clone(),
                    state.clone(),
                    Arc::clone(&pending_removals),
                    id,
                );
            }
            _ => {}
        }
    }

    log::info!("mDNS discovery stream closed; exiting discovery loop");
}

fn schedule_peer_removal(
    app: AppHandle,
    state: AppState,
    pending: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
    id: String,
) {
    let id_for_task = id.clone();
    let pending_for_task = Arc::clone(&pending);
    // `tokio::spawn` is safe here because the discovery loop itself
    // runs inside Tauri's managed tokio runtime, so we always have a
    // reactor in scope when this fn is called.
    let task = tokio::spawn(async move {
        tokio::time::sleep(PEER_REMOVAL_GRACE).await;
        // Self-clean from the pending map first so a later Discovered
        // arriving exactly here doesn't try to abort a finished task.
        pending_for_task.lock().unwrap().remove(&id_for_task);
        if state.remove_peer(&id_for_task).is_some() {
            log::info!("peer expired: {id_for_task}");
            let _ = app.emit("peer-removed", PeerRemoved { id: id_for_task });
        }
    });
    // Newer pending removal supersedes an older one (which would be
    // stale — the peer expired, came back, expired again).
    if let Some(prev) = pending.lock().unwrap().insert(id, task) {
        prev.abort();
    }
}
