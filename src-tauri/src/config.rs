use std::path::PathBuf;

use iroh::SecretKey;
use serde::{Deserialize, Serialize};

use crate::identity;

/// User-configurable settings, persisted as JSON.
///
/// `secret_key` is hex-encoded. The frontend never sees it; only the
/// derived `id` (public key) is exposed via [`crate::identity::Identity`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// 32-byte Ed25519 secret key encoded as 64 lowercase hex chars.
    /// Generated once on first launch and never rotated.
    pub secret_key: String,
    /// User-visible name for this device on the network.
    pub display_name: String,
    /// Directory where incoming files land.
    pub download_dir: String,
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
    fn defaults_for(secret_key_hex: String) -> Self {
        let download_dir = default_download_dir();
        let display_name = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "Yonder Device".to_string());

        Self {
            secret_key: secret_key_hex,
            display_name,
            download_dir: download_dir.to_string_lossy().to_string(),
            auto_accept: false,
            start_minimized: false,
            start_on_login: false,
            theme: "dark".to_string(),
        }
    }

    /// Parse the hex secret key into the typed [`SecretKey`]. Returns
    /// the decode error verbatim — the caller decides whether to
    /// regenerate or surface to the user.
    pub fn secret(&self) -> Result<SecretKey, anyhow::Error> {
        identity::secret_from_hex(&self.secret_key)
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

/// Load settings from disk. If the file is missing, unparseable, or its
/// secret key is malformed we write a fresh default file so subsequent
/// reads succeed.
pub fn load_or_init() -> Settings {
    let path = settings_path();
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(s) = serde_json::from_slice::<Settings>(&bytes) {
            // Validate the secret key parses; otherwise regenerate.
            if s.secret().is_ok() {
                return s;
            }
            log::warn!("settings.json had an unparseable secret_key; regenerating");
        }
    }
    let key = SecretKey::generate();
    let defaults = Settings::defaults_for(identity::secret_to_hex(&key));
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
        log::warn!(
            "could not create download dir {}: {e}",
            settings.download_dir
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Settings {
        let key = SecretKey::generate();
        Settings::defaults_for(identity::secret_to_hex(&key))
    }

    #[test]
    fn defaults_have_sane_baseline() {
        let s = sample();
        assert_eq!(s.secret_key.len(), 64);
        assert!(s.secret().is_ok());
        assert!(!s.auto_accept);
        assert!(!s.start_minimized);
        assert!(!s.start_on_login);
        assert_eq!(s.theme, "dark");
        assert!(!s.display_name.is_empty());
        assert!(s.download_dir.contains("Yonder"));
    }

    #[test]
    fn settings_round_trip_through_json() {
        let s = sample();
        let bytes = serde_json::to_vec(&s).unwrap();
        let back: Settings = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back.secret_key, s.secret_key);
        assert_eq!(back.display_name, s.display_name);
        assert_eq!(back.download_dir, s.download_dir);
        assert_eq!(back.theme, s.theme);
        assert!(back.secret().is_ok());
    }
}
