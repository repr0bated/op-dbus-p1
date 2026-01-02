//! Privacy Router API Handlers
//!
//! Handles user signup, magic link verification, and config download.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};

use crate::state::AppState;
use crate::wireguard::{generate_client_config, generate_keypair, generate_qr_code};

#[derive(Debug, Deserialize)]
pub struct SignupRequest {
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct SignupResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyQuery {
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    pub success: bool,
    pub user_id: Option<String>,
    pub config: Option<String>,
    pub qr_code: Option<String>,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub available: bool,
    pub server_public_key: Option<String>,
    pub endpoint: Option<String>,
    pub registered_users: usize,
}

/// POST /api/privacy/signup - Register with email and send magic link
pub async fn signup(
    State(state): State<Arc<AppState>>,
    Json(request): Json<SignupRequest>,
) -> (StatusCode, Json<SignupResponse>) {
    let email = request.email.trim().to_lowercase();

    // Basic email validation
    if !email.contains('@') || !email.contains('.') {
        return (
            StatusCode::BAD_REQUEST,
            Json(SignupResponse {
                success: false,
                message: "Invalid email address".to_string(),
            }),
        );
    }

    // Check if user already exists
    if let Some(existing) = state.user_store.get_user_by_email(&email).await {
        // User exists - create new magic link
        match state.user_store.create_magic_link(&existing.id).await {
            Ok(link) => {
                // Send email
                if let Err(e) = state.email_sender.send_magic_link(&email, &link.token).await {
                    error!("Failed to send magic link email: {}", e);
                }
                return (
                    StatusCode::OK,
                    Json(SignupResponse {
                        success: true,
                        message: "Check your email for the login link".to_string(),
                    }),
                );
            }
            Err(e) => {
                error!("Failed to create magic link: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(SignupResponse {
                        success: false,
                        message: "Failed to create login link".to_string(),
                    }),
                );
            }
        }
    }

    // New user - generate WireGuard keys
    let keypair = generate_keypair();

    // Create user (we'll encrypt the private key later, for now just store it)
    match state
        .user_store
        .create_user(&email, keypair.public_key, keypair.private_key)
        .await
    {
        Ok(user) => {
            // Create magic link
            match state.user_store.create_magic_link(&user.id).await {
                Ok(link) => {
                    // Send email
                    if let Err(e) = state.email_sender.send_magic_link(&email, &link.token).await {
                        error!("Failed to send magic link email: {}", e);
                    }
                    info!("New privacy user registered: {}", email);
                    (
                        StatusCode::OK,
                        Json(SignupResponse {
                            success: true,
                            message: "Check your email for the login link".to_string(),
                        }),
                    )
                }
                Err(e) => {
                    error!("Failed to create magic link: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(SignupResponse {
                            success: false,
                            message: "Failed to create login link".to_string(),
                        }),
                    )
                }
            }
        }
        Err(e) => {
            error!("Failed to create user: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SignupResponse {
                    success: false,
                    message: format!("Registration failed: {}", e),
                }),
            )
        }
    }
}

/// GET /api/privacy/verify?token=xxx - Verify magic link and return config
pub async fn verify(
    State(state): State<Arc<AppState>>,
    Query(query): Query<VerifyQuery>,
) -> (StatusCode, Json<VerifyResponse>) {
    match state.user_store.verify_magic_link(&query.token).await {
        Ok(user) => {
            // Generate WireGuard config
            let config = generate_client_config(
                &user.wg_private_key_encrypted, // This is the actual private key for now
                &user.assigned_ip,
                &state.server_config,
            );

            // Generate QR code
            let qr_code = generate_qr_code(&config).ok();

            info!("User {} verified and received config", user.id);

            (
                StatusCode::OK,
                Json(VerifyResponse {
                    success: true,
                    user_id: Some(user.id),
                    config: Some(config),
                    qr_code,
                    message: "Welcome! Your VPN configuration is ready.".to_string(),
                }),
            )
        }
        Err(e) => {
            error!("Magic link verification failed: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(VerifyResponse {
                    success: false,
                    user_id: None,
                    config: None,
                    qr_code: None,
                    message: format!("Verification failed: {}", e),
                }),
            )
        }
    }
}

/// GET /api/privacy/config/:user_id - Download config for verified user
pub async fn get_config(
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
) -> (StatusCode, Json<VerifyResponse>) {
    match state.user_store.get_user(&user_id).await {
        Some(user) if user.email_verified => {
            let config = generate_client_config(
                &user.wg_private_key_encrypted,
                &user.assigned_ip,
                &state.server_config,
            );
            let qr_code = generate_qr_code(&config).ok();

            (
                StatusCode::OK,
                Json(VerifyResponse {
                    success: true,
                    user_id: Some(user.id),
                    config: Some(config),
                    qr_code,
                    message: "Configuration retrieved".to_string(),
                }),
            )
        }
        Some(_) => (
            StatusCode::FORBIDDEN,
            Json(VerifyResponse {
                success: false,
                user_id: None,
                config: None,
                qr_code: None,
                message: "Email not verified".to_string(),
            }),
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(VerifyResponse {
                success: false,
                user_id: None,
                config: None,
                qr_code: None,
                message: "User not found".to_string(),
            }),
        ),
    }
}

/// GET /api/privacy/status - Check privacy router availability
pub async fn status(State(state): State<Arc<AppState>>) -> Json<StatusResponse> {
    // For now, just report basic status
    Json(StatusResponse {
        available: !state.server_config.public_key.is_empty(),
        server_public_key: if state.server_config.public_key.is_empty() {
            None
        } else {
            Some(state.server_config.public_key.clone())
        },
        endpoint: Some(state.server_config.endpoint.clone()),
        registered_users: 0, // TODO: Add user count method
    })
}
