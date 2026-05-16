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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_device_id_returns_unique_uuids() {
        let a = new_device_id();
        let b = new_device_id();
        assert_ne!(a, b);
        // Standard hyphenated UUID v4 length
        assert_eq!(a.len(), 36);
    }

    #[test]
    fn identity_uses_explicit_display_name_when_provided() {
        let id = Identity::new("device-1".into(), Some("Alice's Mac".into()));
        assert_eq!(id.id, "device-1");
        assert_eq!(id.name, "Alice's Mac");
        assert_eq!(id.version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn identity_falls_back_to_a_non_empty_name() {
        let id = Identity::new("device-2".into(), None);
        assert!(!id.name.is_empty(), "fallback name should never be blank");
    }

    #[test]
    fn identity_os_tag_is_one_of_the_known_buckets() {
        let id = Identity::new("device-3".into(), Some("X".into()));
        assert!(
            matches!(
                id.os.as_str(),
                "linux" | "macos" | "windows" | "android" | "unknown"
            ),
            "unexpected os tag {:?}",
            id.os
        );
    }
}
