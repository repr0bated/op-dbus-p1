#!/bin/bash
# Quick fix: Add index route to op-web
# Run this on your server

set -e

echo "🔍 Diagnosing op-web frontend issue..."
echo ""

# Check backend routes
echo "Testing backend routes:"
for path in "/" "/index.html" "/chat" "/api/health"; do
    code=$(curl -s -o /dev/null -w '%{http_code}' "http://localhost:8080$path" 2>/dev/null || echo "ERR")
    echo "  http://localhost:8080$path -> $code"
done
echo ""

# Check for static files
echo "Looking for static files:"
for dir in "/var/www/op-dbus" "$HOME/git/op-dbus-v2/static" "$HOME/git/op-dbus-v2/crates/op-web/static"; do
    if [ -d "$dir" ]; then
        echo "  ✅ Found: $dir"
        ls -la "$dir" 2>/dev/null | head -5
    else
        echo "  ❌ Not found: $dir"
    fi
done
echo ""

# Check if op-web has index route in code
echo "Checking op-web source for index route:"
if [ -d "$HOME/git/op-dbus-v2" ]; then
    grep -rn 'route.*"/"' "$HOME/git/op-dbus-v2/crates/op-web/src/" 2>/dev/null | head -5 || echo "  No index route found in source"
fi
echo ""

# Suggestion
echo "📋 DIAGNOSIS:"
if curl -s -o /dev/null -w '%{http_code}' "http://localhost:8080/" | grep -q "404"; then
    echo "  The op-web backend returns 404 for / - it has no index route"
    echo ""
    echo "  OPTIONS:"
    echo "  1. Add index route to op-web source (see routes.rs.patch)"
    echo "  2. Configure nginx to serve static index.html directly"
    echo "  3. Quick fix: Create /var/www/op-dbus/index.html"
else
    code=$(curl -s -o /dev/null -w '%{http_code}' "http://localhost:8080/")
    echo "  Backend returns $code for / - issue may be nginx config"
fi
