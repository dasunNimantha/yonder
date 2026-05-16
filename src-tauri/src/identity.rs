use iroh::{EndpointId, SecretKey};
use serde::{Deserialize, Serialize};

/// Identity for *this* device.
///
/// The endpoint id is the public half of an Ed25519 keypair (the
/// `SecretKey` lives in [`crate::config::Settings`]) and is what other
/// peers see on the wire — it both identifies us and authenticates the
/// QUIC connection. The display name is whatever the user has chosen
/// (defaults to the OS hostname).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    /// `EndpointId` (Ed25519 public key) rendered as a string. Stable
    /// across launches because the secret key is persisted.
    pub id: String,
    pub name: String,
    pub os: String,
    pub version: String,
}

impl Identity {
    pub fn new(secret: &SecretKey, display_name: Option<String>) -> Self {
        let name = display_name.unwrap_or_else(|| {
            hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "Yonder Device".into())
        });

        Self {
            id: secret.public().to_string(),
            name,
            os: detect_os(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// Returns one of `"linux" | "macos" | "windows" | "android" | "unknown"`.
pub fn detect_os() -> String {
    let info = os_info::get();
    match info.os_type() {
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
    .to_string()
}

/// Compact peer label used for tooltips: first 8 hex-ish chars of the id.
pub fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

/// Helper for the Settings round-trip: serialize a SecretKey as 32-byte
/// hex so it survives the JSON file unchanged. We avoid base64 to keep
/// the dependency footprint small (`data-encoding` is also used for
/// node-id rendering).
pub fn secret_to_hex(s: &SecretKey) -> String {
    data_encoding::HEXLOWER.encode(&s.to_bytes())
}

pub fn secret_from_hex(s: &str) -> Result<SecretKey, anyhow::Error> {
    let bytes = data_encoding::HEXLOWER
        .decode(s.as_bytes())
        .map_err(|e| anyhow::anyhow!("invalid hex: {e}"))?;
    let arr: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected 32 bytes, got {}", bytes.len()))?;
    Ok(SecretKey::from_bytes(&arr))
}

/// Convenience: parse a peer-id string back into the typed [`EndpointId`].
pub fn parse_endpoint_id(id: &str) -> Result<EndpointId, anyhow::Error> {
    id.parse::<EndpointId>()
        .map_err(|e| anyhow::anyhow!("invalid endpoint id: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_round_trip_through_hex() {
        let s = SecretKey::generate();
        let hex = secret_to_hex(&s);
        assert_eq!(hex.len(), 64);
        let back = secret_from_hex(&hex).unwrap();
        assert_eq!(back.public().to_string(), s.public().to_string());
    }

    #[test]
    fn secret_from_hex_rejects_garbage() {
        assert!(secret_from_hex("not hex").is_err());
        assert!(secret_from_hex(&"aa".repeat(31)).is_err()); // 31 bytes
    }

    #[test]
    fn identity_uses_explicit_display_name() {
        let s = SecretKey::generate();
        let id = Identity::new(&s, Some("Alice's Mac".into()));
        assert_eq!(id.name, "Alice's Mac");
        assert_eq!(id.id, s.public().to_string());
        assert_eq!(id.version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn identity_falls_back_to_a_non_empty_name() {
        let s = SecretKey::generate();
        let id = Identity::new(&s, None);
        assert!(!id.name.is_empty());
    }

    #[test]
    fn identity_os_tag_is_one_of_the_known_buckets() {
        let s = SecretKey::generate();
        let id = Identity::new(&s, Some("X".into()));
        assert!(matches!(
            id.os.as_str(),
            "linux" | "macos" | "windows" | "android" | "unknown"
        ));
    }

    #[test]
    fn parse_endpoint_id_round_trips() {
        let s = SecretKey::generate();
        let id_str = s.public().to_string();
        let parsed = parse_endpoint_id(&id_str).unwrap();
        assert_eq!(parsed.to_string(), id_str);
    }

    #[test]
    fn short_id_truncates_to_eight() {
        assert_eq!(short_id("0123456789abcdef"), "01234567");
        assert_eq!(short_id("abc"), "abc");
    }
}
