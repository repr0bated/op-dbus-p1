#!/bin/bash
CERT_FILE="/etc/letsencrypt/live/logs.ghostbridge.tech/fullchain.pem"

if [ -f "$CERT_FILE" ]; then
    echo "Certificate found. Testing Nginx config..."
    if sudo nginx -t; then
        echo "Config OK. Reloading Nginx..."
        sudo systemctl reload nginx
        echo "Nginx reloaded."
    else
        echo "Nginx config check failed."
        exit 1
    fi
else
    echo "Certificate not found at $CERT_FILE"
    echo "Cannot enable SSL for logs.ghostbridge.tech yet."
    echo "Please obtain a certificate (e.g., using certbot) or check the path."
    exit 1
fi
