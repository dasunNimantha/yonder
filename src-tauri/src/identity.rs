use serde::{Deserialize, Serialize};

/// Identity for *this* device that we advertise over mDNS and embed in
/// requests. The id is a stable random UUID generated on first launch
/// and persisted in `settings.json`; the name is whatever the user has
/// chosen (defaults to the OS hostname).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub id: String,
    pub name: String,
    pub os: String,
    pub version: String,
}

impl Identity {
    pub fn new(id: String, display_name: Option<String>) -> Self {
        let name = display_name.unwrap_or_else(|| {
            hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "Yonder Device".into())
        });

        let info = os_info::get();
        let os = match info.os_type() {
            os_info::Type::Macos => "macos",
            os_info::Type::Windows => "windows",
            os_info::Type::Android => "android",
            os_info::Type::Linux
            | os_info::Type::Ubuntu
            | os_info::Type::Debian
            | os_info::Type::Fedora
            | os_info::Type::Arch
            | os_info::Type::Alpine
            | os_info::Type::Manjaro
            | os_info::Type::Mint
            | os_info::Type::openSUSE
            | os_info::Type::Pop
            | os_info::Type::EndeavourOS => "linux",
            _ => "unknown",
        }
        .to_string();

        Self {
            id,
            name,
            os,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

pub fn new_device_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
