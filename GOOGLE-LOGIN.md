# Google OAuth Login Implementation

## Overview

Add Google OAuth authentication to op-dbus web interface.

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Browser   │────▶│  op-web     │────▶│   Google    │
│             │◀────│  /auth/*    │◀────│   OAuth     │
└─────────────┘     └─────────────┘     └─────────────┘
                           │
                           ▼
                    ┌─────────────┐
                    │  Session    │
                    │  Store      │
                    └─────────────┘
```

## File Structure

```
crates/op-web/src/
├── auth/
│   ├── mod.rs              # NEW
│   ├── google.rs           # NEW: Google OAuth
│   ├── session.rs          # NEW: Session management
│   └── middleware.rs       # NEW: Auth middleware
├── lib.rs                  # UPDATE: add auth module
└── routes.rs               # UPDATE: add auth routes
```

## Step 1: Add Dependencies

```toml
# crates/op-web/Cargo.toml
[dependencies]
oauth2 = "4.4"
reqwest = { version = "0.11", features = ["json"] }
jsonwebtoken = "9.0"
rand = "0.8"
base64 = "0.21"
tower-cookies = "0.10"
```

## Step 2: Create auth/mod.rs

```rust
pub mod google;
pub mod middleware;
pub mod session;

pub use google::GoogleAuth;
pub use middleware::RequireAuth;
pub use session::{Session, SessionStore};
```

## Step 3: Create auth/google.rs

```rust
use anyhow::Result;
use oauth2::{
    basic::BasicClient, AuthUrl, ClientId, ClientSecret, RedirectUrl,
    TokenResponse, TokenUrl, AuthorizationCode, CsrfToken, Scope,
};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleUser {
    pub id: String,
    pub email: String,
    pub name: String,
    pub picture: Option<String>,
}

pub struct GoogleAuth {
    client: BasicClient,
    http_client: HttpClient,
}

impl GoogleAuth {
    /// Create from environment variables
    pub fn from_env() -> Result<Self> {
        let client_id = std::env::var("GOOGLE_CLIENT_ID")
            .map_err(|_| anyhow::anyhow!("GOOGLE_CLIENT_ID not set"))?;
        let client_secret = std::env::var("GOOGLE_CLIENT_SECRET")
            .map_err(|_| anyhow::anyhow!("GOOGLE_CLIENT_SECRET not set"))?;
        let redirect_url = std::env::var("GOOGLE_REDIRECT_URL")
            .unwrap_or_else(|_| "http://localhost:8080/auth/google/callback".to_string());

        Self::new(&client_id, &client_secret, &redirect_url)
    }

    pub fn new(client_id: &str, client_secret: &str, redirect_url: &str) -> Result<Self> {
        let client = BasicClient::new(
            ClientId::new(client_id.to_string()),
            Some(ClientSecret::new(client_secret.to_string())),
            AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string())?,
            Some(TokenUrl::new("https://oauth2.googleapis.com/token".to_string())?),
        )
        .set_redirect_uri(RedirectUrl::new(redirect_url.to_string())?);

        Ok(Self {
            client,
            http_client: HttpClient::new(),
        })
    }

    /// Generate authorization URL
    pub fn auth_url(&self) -> (String, CsrfToken) {
        let (url, csrf_token) = self.client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new("openid".to_string()))
            .add_scope(Scope::new("email".to_string()))
            .add_scope(Scope::new("profile".to_string()))
            .url();

        (url.to_string(), csrf_token)
    }

    /// Exchange code for token and get user info
    pub async fn authenticate(&self, code: &str) -> Result<GoogleUser> {
        // Exchange code for token
        let token = self.client
            .exchange_code(AuthorizationCode::new(code.to_string()))
            .request_async(oauth2::reqwest::async_http_client)
            .await
            .map_err(|e| anyhow::anyhow!("Token exchange failed: {}", e))?;

        let access_token = token.access_token().secret();

        // Get user info
        let user_info: GoogleUserInfo = self.http_client
            .get("https://www.googleapis.com/oauth2/v2/userinfo")
            .bearer_auth(access_token)
            .send()
            .await?
            .json()
            .await?;

        info!(email = %user_info.email, "Google auth successful");

        Ok(GoogleUser {
            id: user_info.id,
            email: user_info.email,
            name: user_info.name,
            picture: user_info.picture,
        })
    }
}

#[derive(Deserialize)]
struct GoogleUserInfo {
    id: String,
    email: String,
    name: String,
    picture: Option<String>,
}
```

## Step 4: Create auth/session.rs

```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc, Duration};

use super::google::GoogleUser;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub user: GoogleUser,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl Session {
    pub fn new(user: GoogleUser, ttl_hours: i64) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            user,
            created_at: now,
            expires_at: now + Duration::hours(ttl_hours),
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}

pub struct SessionStore {
    sessions: RwLock<HashMap<String, Session>>,
    csrf_tokens: RwLock<HashMap<String, DateTime<Utc>>>,
    session_ttl_hours: i64,
}

impl SessionStore {
    pub fn new(session_ttl_hours: i64) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            csrf_tokens: RwLock::new(HashMap::new()),
            session_ttl_hours,
        }
    }

    /// Store CSRF token for validation
    pub async fn store_csrf(&self, token: &str) {
        let mut tokens = self.csrf_tokens.write().await;
        tokens.insert(token.to_string(), Utc::now());
        
        // Cleanup old tokens (older than 10 minutes)
        let cutoff = Utc::now() - Duration::minutes(10);
        tokens.retain(|_, created| *created > cutoff);
    }

    /// Validate and consume CSRF token
    pub async fn validate_csrf(&self, token: &str) -> bool {
        let mut tokens = self.csrf_tokens.write().await;
        tokens.remove(token).is_some()
    }

    /// Create session for user
    pub async fn create_session(&self, user: GoogleUser) -> Session {
        let session = Session::new(user, self.session_ttl_hours);
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.id.clone(), session.clone());
        session
    }

    /// Get session by ID
    pub async fn get_session(&self, id: &str) -> Option<Session> {
        let sessions = self.sessions.read().await;
        sessions.get(id).cloned().filter(|s| !s.is_expired())
    }

    /// Delete session
    pub async fn delete_session(&self, id: &str) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(id);
    }

    /// Cleanup expired sessions
    pub async fn cleanup(&self) {
        let mut sessions = self.sessions.write().await;
        sessions.retain(|_, s| !s.is_expired());
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new(24) // 24 hour sessions
    }
}
```

## Step 5: Create auth/middleware.rs

```rust
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use std::sync::Arc;
use tower_cookies::Cookies;

use super::session::SessionStore;

pub const SESSION_COOKIE: &str = "op_session";

/// Middleware to require authentication
pub async fn require_auth(
    cookies: Cookies,
    State(sessions): State<Arc<SessionStore>>,
    request: Request,
    next: Next,
) -> Response {
    let session_id = cookies
        .get(SESSION_COOKIE)
        .map(|c| c.value().to_string());

    match session_id {
        Some(id) if sessions.get_session(&id).await.is_some() => {
            next.run(request).await
        }
        _ => {
            // Redirect to login
            Redirect::to("/auth/google/login").into_response()
        }
    }
}

/// Middleware to optionally extract user (doesn't require auth)
pub async fn optional_auth(
    cookies: Cookies,
    State(sessions): State<Arc<SessionStore>>,
    mut request: Request,
    next: Next,
) -> Response {
    if let Some(session_id) = cookies.get(SESSION_COOKIE).map(|c| c.value().to_string()) {
        if let Some(session) = sessions.get_session(&session_id).await {
            request.extensions_mut().insert(session);
        }
    }
    next.run(request).await
}
```

## Step 6: Create Auth Routes

```rust
// crates/op-web/src/auth_routes.rs
use axum::{
    extract::{Query, State},
    response::{IntoResponse, Redirect},
    routing::get,
    Router,
};
use serde::Deserialize;
use std::sync::Arc;
use tower_cookies::{Cookie, Cookies};
use tracing::{error, info};

use crate::auth::{GoogleAuth, SessionStore, middleware::SESSION_COOKIE};

pub struct AuthState {
    pub google: GoogleAuth,
    pub sessions: Arc<SessionStore>,
}

pub fn auth_routes(state: Arc<AuthState>) -> Router {
    Router::new()
        .route("/auth/google/login", get(google_login))
        .route("/auth/google/callback", get(google_callback))
        .route("/auth/logout", get(logout))
        .route("/auth/me", get(me))
        .with_state(state)
}

async fn google_login(
    State(state): State<Arc<AuthState>>,
) -> impl IntoResponse {
    let (url, csrf_token) = state.google.auth_url();
    
    // Store CSRF for validation
    state.sessions.store_csrf(csrf_token.secret()).await;
    
    Redirect::to(&url)
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: String,
    state: String,
}

async fn google_callback(
    cookies: Cookies,
    State(state): State<Arc<AuthState>>,
    Query(query): Query<CallbackQuery>,
) -> impl IntoResponse {
    // Validate CSRF
    if !state.sessions.validate_csrf(&query.state).await {
        error!("Invalid CSRF token");
        return Redirect::to("/auth/error?reason=csrf").into_response();
    }

    // Exchange code for user info
    match state.google.authenticate(&query.code).await {
        Ok(user) => {
            info!(email = %user.email, "User logged in");
            
            // Create session
            let session = state.sessions.create_session(user).await;
            
            // Set cookie
            let cookie = Cookie::build((SESSION_COOKIE, session.id))
                .path("/")
                .http_only(true)
                .secure(true)  // Set to false for local dev
                .same_site(tower_cookies::cookie::SameSite::Lax)
                .max_age(tower_cookies::cookie::time::Duration::hours(24));
            
            cookies.add(cookie);
            
            Redirect::to("/").into_response()
        }
        Err(e) => {
            error!(error = %e, "Google auth failed");
            Redirect::to("/auth/error?reason=google").into_response()
        }
    }
}

async fn logout(
    cookies: Cookies,
    State(state): State<Arc<AuthState>>,
) -> impl IntoResponse {
    if let Some(session_id) = cookies.get(SESSION_COOKIE).map(|c| c.value().to_string()) {
        state.sessions.delete_session(&session_id).await;
    }
    
    cookies.remove(Cookie::from(SESSION_COOKIE));
    
    Redirect::to("/")
}

async fn me(
    cookies: Cookies,
    State(state): State<Arc<AuthState>>,
) -> impl IntoResponse {
    let session_id = cookies.get(SESSION_COOKIE).map(|c| c.value().to_string());
    
    match session_id {
        Some(id) => match state.sessions.get_session(&id).await {
            Some(session) => axum::Json(serde_json::json!({
                "authenticated": true,
                "user": session.user
            })).into_response(),
            None => axum::Json(serde_json::json!({
                "authenticated": false
            })).into_response(),
        },
        None => axum::Json(serde_json::json!({
            "authenticated": false
        })).into_response(),
    }
}
```

## Step 7: Environment Variables

```bash
# /etc/op-dbus/environment
GOOGLE_CLIENT_ID=your-client-id.apps.googleusercontent.com
GOOGLE_CLIENT_SECRET=your-client-secret
GOOGLE_REDIRECT_URL=https://your-domain.com/auth/google/callback
```

## Step 8: Google Cloud Console Setup

1. Go to [Google Cloud Console](https://console.cloud.google.com/)
2. Create new project or select existing
3. Enable "Google+ API" or "Google Identity" API
4. Go to "Credentials" → "Create Credentials" → "OAuth 2.0 Client ID"
5. Configure consent screen
6. Add authorized redirect URIs:
   - `http://localhost:8080/auth/google/callback` (dev)
   - `https://your-domain.com/auth/google/callback` (prod)
7. Copy Client ID and Client Secret

## Usage in Routes

```rust
// Protect routes with auth middleware
use axum::middleware;

let protected_routes = Router::new()
    .route("/admin", get(admin_handler))
    .route("/settings", get(settings_handler))
    .layer(middleware::from_fn_with_state(
        sessions.clone(),
        require_auth,
    ));
```
