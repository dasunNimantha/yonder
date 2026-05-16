use anyhow::{anyhow, Context, Result};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::{AppState, Peer};

pub const SERVICE_TYPE: &str = "_yonder._tcp.local.";

#[derive(Debug, Clone, Serialize)]
struct PeerRemoved {
    id: String,
}

/// Hold-onto-it handle for the mDNS daemon. Dropping this is not enough
/// to fully unregister; call [`Discovery::shutdown`] explicitly during
/// app exit.
pub struct Discovery {
    daemon: ServiceDaemon,
    instance_name: String,
}

impl Discovery {
    /// Start advertising our service AND subscribe to the same service
    /// type so we discover other peers in real time. Discovered peers
    /// are funneled into `AppState` and surfaced to the frontend as
    /// `peer-added` / `peer-removed` events.
    pub fn start(handle: AppHandle, port: u16) -> Result<Self> {
        let daemon =
            ServiceDaemon::new().map_err(|e| anyhow!("mdns daemon create failed: {e}"))?;

        let state = handle.state::<AppState>().inner().clone();
        let identity = state.identity();

        let hostname = format!("{}.local.", short_host_id(&identity.id));
        let instance_name = identity.id.clone();

        let info = build_service_info(&instance_name, &hostname, &identity, port)?;

        daemon
            .register(info)
            .map_err(|e| anyhow!("mdns register failed: {e}"))?;

        let receiver = daemon
            .browse(SERVICE_TYPE)
            .map_err(|e| anyhow!("mdns browse failed: {e}"))?;

        let app_for_task = handle.clone();
        let our_id = identity.id.clone();

        tokio::spawn(async move {
            let app = app_for_task;
            while let Ok(event) = receiver.recv_async().await {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        if let Some(peer) = peer_from_info(&info) {
                            if peer.id == our_id {
                                continue;
                            }
                            log::info!(
                                "peer resolved: {} ({}:{})",
                                peer.name,
                                peer.host,
                                peer.port
                            );
                            let state = app.state::<AppState>();
                            let inserted = state.insert_peer(peer.clone());
                            if inserted {
                                let _ = app.emit("peer-added", &peer);
                            } else {
                                let _ = app.emit("peer-updated", &peer);
                            }
                        }
                    }
                    ServiceEvent::ServiceRemoved(_ty, fullname) => {
                        let id = instance_from_fullname(&fullname);
                        if id == our_id {
                            continue;
                        }
                        log::info!("peer removed: {id}");
                        let state = app.state::<AppState>();
                        if state.remove_peer(&id).is_some() {
                            let _ = app.emit("peer-removed", PeerRemoved { id });
                        }
                    }
                    _ => {}
                }
            }
            log::info!("mDNS browse channel closed; exiting discovery task");
        });

        Ok(Self {
            daemon,
            instance_name,
        })
    }

    /// Re-register with updated TXT records (e.g. display name change).
    /// mdns-sd treats a second `register` call as a re-announce, so no
    /// explicit unregister is needed.
    pub fn republish(&self, handle: &AppHandle, port: u16) -> Result<()> {
        let state = handle.state::<AppState>().inner().clone();
        let identity = state.identity();
        let hostname = format!("{}.local.", short_host_id(&identity.id));
        let info = build_service_info(&self.instance_name, &hostname, &identity, port)?;
        self.daemon
            .register(info)
            .map_err(|e| anyhow!("mdns re-register failed: {e}"))?;
        Ok(())
    }

    pub fn shutdown(&self) {
        if let Ok(_rx) = self
            .daemon
            .unregister(&format!("{}.{}", self.instance_name, SERVICE_TYPE))
        {
            // Best-effort wait for the goodbye packet to go out.
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let _ = self.daemon.shutdown();
    }
}

fn build_service_info(
    instance: &str,
    hostname: &str,
    identity: &crate::identity::Identity,
    port: u16,
) -> Result<ServiceInfo> {
    let local_ip = local_ip_address::local_ip()
        .map_err(|e| anyhow!("could not resolve local IP: {e}"))?;

    let mut properties = std::collections::HashMap::new();
    properties.insert("id".to_string(), identity.id.clone());
    properties.insert("name".to_string(), identity.name.clone());
    properties.insert("os".to_string(), identity.os.clone());
    properties.insert("v".to_string(), identity.version.clone());

    let info = ServiceInfo::new(
        SERVICE_TYPE,
        instance,
        hostname,
        local_ip.to_string(),
        port,
        Some(properties),
    )
    .context("invalid ServiceInfo")?;
    Ok(info)
}

fn peer_from_info(info: &ServiceInfo) -> Option<Peer> {
    let id = info.get_property_val_str("id")?.to_string();
    let name = info
        .get_property_val_str("name")
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Unknown device".to_string());
    let os = info
        .get_property_val_str("os")
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let version = info
        .get_property_val_str("v")
        .map(|s| s.to_string())
        .unwrap_or_default();
    let host = info
        .get_addresses()
        .iter()
        .next()
        .map(|a| a.to_string())
        .unwrap_or_else(|| info.get_hostname().to_string());
    let port = info.get_port();

    Some(Peer {
        id,
        name,
        os,
        host,
        port,
        version,
    })
}

/// Service hostnames must be valid DNS labels. The UUID has dashes that
/// are fine, but we keep the host short to keep the multicast packets
/// small.
fn short_host_id(id: &str) -> String {
    id.split('-').next().unwrap_or(id).to_string()
}

fn instance_from_fullname(fullname: &str) -> String {
    fullname
        .strip_suffix(&format!(".{}", SERVICE_TYPE))
        .unwrap_or(fullname)
        .to_string()
}
