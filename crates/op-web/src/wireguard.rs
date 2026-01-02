//! WireGuard Configuration Generation
//!
//! Generates WireGuard client configurations and QR codes for the privacy router.

use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use image::Luma;
use qrcode::QrCode;
use rand::rngs::OsRng;
use x25519_dalek::{PublicKey, StaticSecret};

/// WireGuard keypair
#[derive(Debug, Clone)]
pub struct WgKeyPair {
    pub private_key: String,
    pub public_key: String,
}

/// Generate a new WireGuard keypair
pub fn generate_keypair() -> WgKeyPair {
    let secret = StaticSecret::random_from_rng(OsRng);
    let public = PublicKey::from(&secret);

    WgKeyPair {
        private_key: BASE64.encode(secret.as_bytes()),
        public_key: BASE64.encode(public.as_bytes()),
    }
}

/// WireGuard server configuration for generating client configs
#[derive(Debug, Clone)]
pub struct WgServerConfig {
    pub public_key: String,
    pub endpoint: String,
    pub allowed_ips: String,
    pub dns: String,
}

impl Default for WgServerConfig {
    fn default() -> Self {
        Self {
            // This should be loaded from the actual WireGuard gateway
            public_key: String::new(),
            endpoint: "0.0.0.0:51820".to_string(),
            allowed_ips: "0.0.0.0/0, ::/0".to_string(),
            dns: "1.1.1.1, 1.0.0.1".to_string(),
        }
    }
}

/// Generate a WireGuard client configuration
pub fn generate_client_config(
    client_private_key: &str,
    client_address: &str,
    server: &WgServerConfig,
) -> String {
    format!(
        r#"[Interface]
PrivateKey = {}
Address = {}
DNS = {}

[Peer]
PublicKey = {}
AllowedIPs = {}
Endpoint = {}
PersistentKeepalive = 25
"#,
        client_private_key,
        client_address,
        server.dns,
        server.public_key,
        server.allowed_ips,
        server.endpoint
    )
}

/// Generate a QR code image as base64 PNG
pub fn generate_qr_code(config: &str) -> Result<String> {
    let code = QrCode::new(config.as_bytes())?;
    let image = code.render::<Luma<u8>>().build();

    // Encode as PNG using the image crate's write interface
    let mut png_bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut png_bytes);
    image.write_to(&mut cursor, image::ImageFormat::Png)?;

    // Return as data URL
    Ok(format!("data:image/png;base64,{}", BASE64.encode(&png_bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let keypair = generate_keypair();
        assert!(!keypair.private_key.is_empty());
        assert!(!keypair.public_key.is_empty());
        // Base64 encoded 32-byte key = 44 chars
        assert_eq!(keypair.private_key.len(), 44);
        assert_eq!(keypair.public_key.len(), 44);
    }

    #[test]
    fn test_config_generation() {
        let config = generate_client_config(
            "test_private_key",
            "10.100.0.2/32",
            &WgServerConfig {
                public_key: "server_pub_key".to_string(),
                endpoint: "vpn.example.com:51820".to_string(),
                ..Default::default()
            },
        );
        assert!(config.contains("PrivateKey = test_private_key"));
        assert!(config.contains("Address = 10.100.0.2/32"));
        assert!(config.contains("PublicKey = server_pub_key"));
    }
}
