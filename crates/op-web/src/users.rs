//! Privacy Router User Storage
//!
//! Manages user accounts for the privacy router VPN service.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// A privacy router user account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyUser {
    pub id: String,
    pub email: String,
    pub email_verified: bool,
    pub created_at: DateTime<Utc>,
    pub wg_public_key: String,
    pub wg_private_key_encrypted: String,
    pub assigned_ip: String,
}

/// A magic link for email verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MagicLink {
    pub token: String,
    pub user_id: String,
    pub expires_at: DateTime<Utc>,
}

/// User storage with persistence
pub struct UserStore {
    users: RwLock<HashMap<String, PrivacyUser>>,
    users_by_email: RwLock<HashMap<String, String>>, // email -> user_id
    magic_links: RwLock<HashMap<String, MagicLink>>,
    next_ip: RwLock<u8>, // Last octet for IP assignment (10.100.0.x)
    storage_path: String,
}

impl UserStore {
    /// Create a new user store with persistence path
    pub async fn new(storage_path: impl Into<String>) -> Result<Self> {
        let storage_path = storage_path.into();
        let store = Self {
            users: RwLock::new(HashMap::new()),
            users_by_email: RwLock::new(HashMap::new()),
            magic_links: RwLock::new(HashMap::new()),
            next_ip: RwLock::new(2), // Start at 10.100.0.2
            storage_path,
        };

        // Load existing users if file exists
        store.load().await.ok();

        Ok(store)
    }

    /// Load users from disk
    async fn load(&self) -> Result<()> {
        let path = Path::new(&self.storage_path);
        if !path.exists() {
            return Ok(());
        }

        let content = tokio::fs::read_to_string(path).await?;
        let data: StoredData = serde_json::from_str(&content)?;

        let mut users = self.users.write().await;
        let mut users_by_email = self.users_by_email.write().await;
        let mut next_ip = self.next_ip.write().await;

        for user in data.users {
            users_by_email.insert(user.email.clone(), user.id.clone());
            users.insert(user.id.clone(), user);
        }

        *next_ip = data.next_ip;
        info!("Loaded {} users from {}", users.len(), self.storage_path);

        Ok(())
    }

    /// Save users to disk
    async fn save(&self) -> Result<()> {
        let users = self.users.read().await;
        let next_ip = self.next_ip.read().await;

        let data = StoredData {
            users: users.values().cloned().collect(),
            next_ip: *next_ip,
        };

        let content = serde_json::to_string_pretty(&data)?;

        // Ensure parent directory exists
        if let Some(parent) = Path::new(&self.storage_path).parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }

        tokio::fs::write(&self.storage_path, content).await?;
        Ok(())
    }

    /// Get the next available IP address
    pub async fn allocate_ip(&self) -> String {
        let mut next_ip = self.next_ip.write().await;
        let ip = format!("10.100.0.{}/32", *next_ip);
        *next_ip = next_ip.wrapping_add(1);
        if *next_ip < 2 {
            *next_ip = 2; // Wrap around but skip .0 and .1
        }
        ip
    }

    /// Create a new user (unverified)
    pub async fn create_user(
        &self,
        email: &str,
        wg_public_key: String,
        wg_private_key_encrypted: String,
    ) -> Result<PrivacyUser> {
        // Check if email already exists
        {
            let users_by_email = self.users_by_email.read().await;
            if users_by_email.contains_key(email) {
                anyhow::bail!("Email already registered");
            }
        }

        let ip = self.allocate_ip().await;
        let user = PrivacyUser {
            id: uuid::Uuid::new_v4().to_string(),
            email: email.to_string(),
            email_verified: false,
            created_at: Utc::now(),
            wg_public_key,
            wg_private_key_encrypted,
            assigned_ip: ip,
        };

        // Store user
        {
            let mut users = self.users.write().await;
            let mut users_by_email = self.users_by_email.write().await;
            users_by_email.insert(user.email.clone(), user.id.clone());
            users.insert(user.id.clone(), user.clone());
        }

        // Persist
        self.save().await.context("Failed to save user")?;

        info!("Created user {} with IP {}", user.id, user.assigned_ip);
        Ok(user)
    }

    /// Create a magic link for a user
    pub async fn create_magic_link(&self, user_id: &str) -> Result<MagicLink> {
        use rand::Rng;

        // Generate random token
        let token: String = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();

        let link = MagicLink {
            token: token.clone(),
            user_id: user_id.to_string(),
            expires_at: Utc::now() + Duration::minutes(15),
        };

        let mut links = self.magic_links.write().await;
        links.insert(token, link.clone());

        Ok(link)
    }

    /// Verify a magic link and mark user as verified
    pub async fn verify_magic_link(&self, token: &str) -> Result<PrivacyUser> {
        // Find and remove the magic link
        let link = {
            let mut links = self.magic_links.write().await;
            links.remove(token).context("Invalid or expired link")?
        };

        // Check expiration
        if link.expires_at < Utc::now() {
            anyhow::bail!("Link has expired");
        }

        // Mark user as verified
        let user = {
            let mut users = self.users.write().await;
            let user = users.get_mut(&link.user_id).context("User not found")?;
            user.email_verified = true;
            user.clone()
        };

        // Persist
        self.save().await?;

        info!("User {} verified via magic link", user.id);
        Ok(user)
    }

    /// Get user by ID
    pub async fn get_user(&self, user_id: &str) -> Option<PrivacyUser> {
        let users = self.users.read().await;
        users.get(user_id).cloned()
    }

    /// Get user by email
    pub async fn get_user_by_email(&self, email: &str) -> Option<PrivacyUser> {
        let users_by_email = self.users_by_email.read().await;
        let user_id = users_by_email.get(email)?;
        let users = self.users.read().await;
        users.get(user_id).cloned()
    }

    /// Clean up expired magic links
    pub async fn cleanup_expired_links(&self) {
        let mut links = self.magic_links.write().await;
        let now = Utc::now();
        let before = links.len();
        links.retain(|_, v| v.expires_at > now);
        let removed = before - links.len();
        if removed > 0 {
            warn!("Cleaned up {} expired magic links", removed);
        }
    }
}

#[derive(Serialize, Deserialize)]
struct StoredData {
    users: Vec<PrivacyUser>,
    next_ip: u8,
}
