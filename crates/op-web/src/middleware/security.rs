use axum::{
    extract::{ConnectInfo, Request},
    http::HeaderMap,
    middleware::Next,
    response::Response,
};
use op_core::security::AccessZone;
use std::net::SocketAddr;
use tracing::debug;

/// Extract IP from headers or connection info
pub fn extract_ip(headers: &HeaderMap, addr: Option<&SocketAddr>) -> String {
    // 1. Check X-Forwarded-For (standard proxy header)
    if let Some(forwarded) = headers.get("x-forwarded-for") {
        if let Ok(s) = forwarded.to_str() {
            if let Some(client_ip) = s.split(',').next() {
                return client_ip.trim().to_string();
            }
        }
    }

    // 2. Check X-Real-IP (nginx convention)
    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(s) = real_ip.to_str() {
            return s.trim().to_string();
        }
    }

    // 3. Fallback to socket address
    if let Some(addr) = addr {
        return addr.ip().to_string();
    }

    "0.0.0.0".to_string()
}

/// Middleware to identify Client IP and attach AccessZone to the request
pub async fn ip_security_middleware(
    // We try to extract ConnectInfo if available (requires Router to be constructed with it)
    // If running behind Nginx, this might not be strictly necessary if headers are present,
    // but Axum extraction can be tricky if the type doesn't match.
    // Making it optional avoids runtime panics if it's missing.
    connect_info: Option<ConnectInfo<SocketAddr>>,
    mut request: Request,
    next: Next,
) -> Response {
    let headers = request.headers();
    let addr = connect_info.map(|ci| ci.0);
    
    let client_ip = extract_ip(headers, addr.as_ref());
    let zone = AccessZone::from_ip(&client_ip);

    debug!("Request from IP: {} [Zone: {:?}]", client_ip, zone);

    // Attach AccessZone to the request extensions
    // This allows downstream handlers (like MCP tools) to retrieve it via:
    // Extension(zone): Extension<AccessZone>
    request.extensions_mut().insert(zone);

    next.run(request).await
}
