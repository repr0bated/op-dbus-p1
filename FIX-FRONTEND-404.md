# Fix Frontend 404 at op-dbus.ghostbridge.tech

## Problem

- https://op-dbus.ghostbridge.tech/ returns **404**
- https://op-dbus.ghostbridge.tech/api/health returns **200** ✅
- https://op-dbus.ghostbridge.tech/mcp/agents returns **200** ✅

This means nginx is working, but the **root path** isn't being handled.

## Root Cause Options

### Option A: op-web doesn't serve `/`

The `op-web-server` might not have a route for `/`. Check:

```bash
# Test backend directly
curl -I http://localhost:8080/
curl -I http://localhost:8080/index.html
curl -I http://localhost:8080/chat
```

If backend returns 404, the issue is in **op-web code**, not nginx.

### Option B: Nginx config wrong

Nginx might be configured to only proxy `/api/` and `/mcp/`, not `/`.

```bash
# Check nginx config
cat /etc/nginx/sites-enabled/* | grep -A10 'location /'
```

### Option C: Static files missing

If op-web expects to serve static files from a directory:

```bash
# Check if static dir exists
ls -la /var/www/op-dbus/static/ 2>/dev/null || echo "Not found"
ls -la ~/git/op-dbus-v2/static/ 2>/dev/null || echo "Not found"
```

## Quick Fix

### Step 1: Check Backend

```bash
curl -v http://localhost:8080/
```

If this returns 404, need to fix op-web.

### Step 2: Add Index Route to op-web

In `crates/op-web/src/main.rs` or router, add:

```rust
// Serve index page at /
async fn index_handler() -> impl IntoResponse {
    Html(include_str!("../../../static/index.html"))
}

// In router setup:
Router::new()
    .route("/", get(index_handler))
    // ... rest of routes
```

### Step 3: Or Configure Nginx to Serve Static Files

Add to nginx config:

```nginx
# Serve static files directly
location / {
    root /home/jeremy/git/op-dbus-v2/static;
    try_files $uri $uri/ @backend;
}

location @backend {
    proxy_pass http://127.0.0.1:8080;
    # ... proxy headers
}
```

### Step 4: Restart

```bash
sudo systemctl restart op-web
sudo nginx -t && sudo systemctl reload nginx
```

## Verify

```bash
# Should return 200 now
curl -I https://op-dbus.ghostbridge.tech/
```

## Check op-web Code

Look at the router setup in `crates/op-web/src/main.rs`:

```bash
grep -n "route.*\"/\"" crates/op-web/src/*.rs
grep -n "index" crates/op-web/src/*.rs
```

If there's no index route, you need to add one.
