use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::identity;

/// User-configurable settings, persisted as JSON. Anything that should
/// survive a restart goes here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Stable device id (generated once on first launch).
    pub device_id: String,
    /// User-visible name for this device on the network.
    pub display_name: String,
    /// Directory where incoming files land.
    pub download_dir: String,
    /// TCP port for the HTTP receive server (also advertised via mDNS).
    pub tcp_port: u16,
    /// When true, skip the "Accept?" prompt and accept all incoming
    /// transfers from any peer automatically.
    pub auto_accept: bool,
    /// Start the window hidden (tray-only) at launch.
    pub start_minimized: bool,
    /// Register with the OS to start on login.
    pub start_on_login: bool,
    /// "dark" | "light"
    pub theme: String,
}

impl Settings {
    fn defaults_for(device_id: String) -> Self {
        let download_dir = default_download_dir();
        let display_name = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "Yonder Device".to_string());

        Self {
            device_id,
            display_name,
            download_dir: download_dir.to_string_lossy().to_string(),
            tcp_port: 53317,
            auto_accept: false,
            start_minimized: false,
            start_on_login: false,
            theme: "dark".to_string(),
        }
    }
}

fn default_download_dir() -> PathBuf {
    let base = dirs::download_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Downloads")
    });
    base.join("Yonder")
}

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".config")
        })
        .join("yonder")
}

fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

/// Load settings from disk. If the file is missing or unparseable we
/// write a fresh default file so subsequent reads succeed.
pub fn load_or_init() -> Settings {
    let path = settings_path();
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(s) = serde_json::from_slice::<Settings>(&bytes) {
            return s;
        }
    }
    let defaults = Settings::defaults_for(identity::new_device_id());
    let _ = save(&defaults);
    defaults
}

pub fn save(settings: &Settings) -> std::io::Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let path = settings_path();
    let json = serde_json::to_vec_pretty(settings).unwrap_or_default();
    std::fs::write(path, json)?;
    if let Err(e) = std::fs::create_dir_all(&settings.download_dir) {
        log::warn!("could not create download dir {}: {e}", settings.download_dir);
    }
    Ok(())
}
