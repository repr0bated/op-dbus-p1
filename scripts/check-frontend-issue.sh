#!/bin/bash
# Diagnose why frontend returns 404

echo "🔍 Diagnosing op-dbus frontend 404 issue"
echo ""

# 1. Check what op-web is serving at /
echo "1️⃣ Backend response at /:"
curl -s -I http://localhost:8080/ | head -5
echo ""

# 2. Check if there's a static directory
echo "2️⃣ Looking for static files:"
for dir in "/var/www/op-dbus" "/home/jeremy/git/op-dbus-v2/static" "/home/jeremy/op-dbus-v2/static" "./static"; do
    if [ -d "$dir" ]; then
        echo "   ✅ Found: $dir"
        ls -la "$dir" | head -5
    fi
done
echo ""

# 3. Check op-web routes
echo "3️⃣ Testing backend routes:"
for path in "/" "/index.html" "/chat" "/chat.html" "/api/health"; do
    code=$(curl -s -o /dev/null -w '%{http_code}' "http://localhost:8080$path")
    echo "   $path -> $code"
done
echo ""

# 4. Check nginx config
echo "4️⃣ Nginx configuration:"
cat /etc/nginx/sites-enabled/* 2>/dev/null | grep -A5 "location /" | head -20
echo ""

# 5. Check what the nginx is actually doing
echo "5️⃣ Nginx access log (last 5 requests):"
sudo tail -5 /var/log/nginx/*access*.log 2>/dev/null || echo "   No access logs found"
echo ""

echo "6️⃣ Nginx error log (last 5 errors):"
sudo tail -5 /var/log/nginx/*error*.log 2>/dev/null || echo "   No error logs found"
