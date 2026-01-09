#!/bin/bash
# Fix nginx to serve frontend at root path
# Run as root on the server

set -e

echo "🔧 Fixing nginx frontend routing for op-dbus.ghostbridge.tech"
echo ""

# Check current situation
echo "📋 Current status:"
echo "  Health endpoint: $(curl -s -o /dev/null -w '%{http_code}' https://op-dbus.ghostbridge.tech/api/health)"
echo "  Root path:       $(curl -s -o /dev/null -w '%{http_code}' https://op-dbus.ghostbridge.tech/)"
echo "  MCP agents:      $(curl -s -o /dev/null -w '%{http_code}' https://op-dbus.ghostbridge.tech/mcp/agents)"
echo ""

# Find and backup current nginx config
NGINX_CONF="/etc/nginx/sites-available/op-dbus-final.conf"
if [ ! -f "$NGINX_CONF" ]; then
    NGINX_CONF="/etc/nginx/sites-available/op-web"
fi
if [ ! -f "$NGINX_CONF" ]; then
    NGINX_CONF="/etc/nginx/sites-available/op-dbus"
fi

if [ ! -f "$NGINX_CONF" ]; then
    echo "❌ Cannot find nginx config file"
    echo "   Checked: op-dbus-final.conf, op-web, op-dbus"
    exit 1
fi

echo "📄 Using nginx config: $NGINX_CONF"
cp "$NGINX_CONF" "${NGINX_CONF}.backup.$(date +%Y%m%d%H%M%S)"

# Check what backend port op-web is listening on
BACKEND_PORT=$(ss -tlnp | grep op-web | grep -oP ':\K[0-9]+' | head -1)
if [ -z "$BACKEND_PORT" ]; then
    echo "⚠️  op-web not detected, checking for listening ports..."
    ss -tlnp | grep -E ':808[0-9]'
    BACKEND_PORT="8080"  # Default
fi
echo "📡 Backend port: $BACKEND_PORT"

# Create fixed nginx config
cat > "$NGINX_CONF" << 'NGINX_EOF'
# op-dbus.ghostbridge.tech nginx configuration
# Fixed: Serves frontend at root path

upstream op_web_backend {
    server 127.0.0.1:8080;
    keepalive 32;
}

# HTTP redirect to HTTPS
server {
    listen 80;
    listen [::]:80;
    server_name op-dbus.ghostbridge.tech;
    return 301 https://$host$request_uri;
}

# HTTPS server
server {
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name op-dbus.ghostbridge.tech;

    # SSL - use existing certs
    ssl_certificate /etc/letsencrypt/live/op-dbus.ghostbridge.tech/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/op-dbus.ghostbridge.tech/privkey.pem;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_prefer_server_ciphers off;

    # Logging
    access_log /var/log/nginx/op-dbus-access.log;
    error_log /var/log/nginx/op-dbus-error.log;

    # Root - proxy to backend (backend serves index.html)
    location / {
        proxy_pass http://op_web_backend;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_read_timeout 86400;
    }

    # API endpoints
    location /api/ {
        proxy_pass http://op_web_backend/api/;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # MCP endpoints
    location /mcp/ {
        proxy_pass http://op_web_backend/mcp/;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        
        # SSE support
        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 86400;
    }

    # Health check (direct)
    location = /health {
        proxy_pass http://op_web_backend/api/health;
    }
}
NGINX_EOF

# Update port if different
if [ "$BACKEND_PORT" != "8080" ]; then
    sed -i "s/127.0.0.1:8080/127.0.0.1:$BACKEND_PORT/g" "$NGINX_CONF"
    echo "📝 Updated backend port to $BACKEND_PORT"
fi

# Check for SSL cert paths - use what exists
if [ -f "/etc/letsencrypt/live/op-dbus.ghostbridge.tech/fullchain.pem" ]; then
    echo "✅ Using Let's Encrypt cert"
elif [ -f "/etc/nginx/ssl/ghostbridge.crt" ]; then
    echo "📝 Switching to existing ssl cert"
    sed -i 's|/etc/letsencrypt/live/op-dbus.ghostbridge.tech/fullchain.pem|/etc/nginx/ssl/ghostbridge.crt|g' "$NGINX_CONF"
    sed -i 's|/etc/letsencrypt/live/op-dbus.ghostbridge.tech/privkey.pem|/etc/nginx/ssl/ghostbridge.key|g' "$NGINX_CONF"
else
    echo "⚠️  No SSL cert found - you may need to configure this"
fi

# Ensure site is enabled
ln -sf "$NGINX_CONF" /etc/nginx/sites-enabled/op-dbus.conf 2>/dev/null || true

# Test and reload nginx
echo ""
echo "🔄 Testing nginx config..."
if nginx -t 2>&1; then
    echo "✅ Nginx config valid"
    systemctl reload nginx
    echo "✅ Nginx reloaded"
else
    echo "❌ Nginx config invalid"
    echo "   Restoring backup..."
    cp "${NGINX_CONF}.backup."* "$NGINX_CONF" 2>/dev/null || true
    exit 1
fi

sleep 2

# Test again
echo ""
echo "📋 Testing endpoints:"
for path in "/" "/api/health" "/mcp/agents"; do
    code=$(curl -s -o /dev/null -w '%{http_code}' "https://op-dbus.ghostbridge.tech$path" 2>/dev/null || echo "ERR")
    if [ "$code" = "200" ]; then
        echo "  ✅ $path -> $code"
    elif [ "$code" = "301" ] || [ "$code" = "302" ]; then
        echo "  ↪️  $path -> $code (redirect)"
    else
        echo "  ❌ $path -> $code"
    fi
done

echo ""
echo "🎉 Done! Try: https://op-dbus.ghostbridge.tech/"
