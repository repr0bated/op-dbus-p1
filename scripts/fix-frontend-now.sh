#!/bin/bash
# QUICK FIX: Create frontend and configure nginx
# Run as root on your server

set -e

echo "🔧 Quick Frontend Fix"
echo ""

# Create static directory
STATIC_DIR="/var/www/op-dbus"
mkdir -p "$STATIC_DIR"

# Create index.html
cat > "$STATIC_DIR/index.html" << 'HTML'
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>OP-DBUS Chat</title>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body { 
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: #1a1a2e; 
            color: #eee; 
            height: 100vh;
            display: flex;
            flex-direction: column;
        }
        header {
            background: #16213e;
            padding: 1rem 2rem;
            border-bottom: 1px solid #0f3460;
            display: flex;
            justify-content: space-between;
            align-items: center;
        }
        header h1 { font-size: 1.5rem; color: #e94560; }
        .status { display: flex; align-items: center; gap: 0.5rem; font-size: 0.9rem; }
        .status-dot { width: 10px; height: 10px; border-radius: 50%; background: #4ade80; }
        .status-dot.error { background: #f87171; }
        main { flex: 1; display: flex; flex-direction: column; overflow: hidden; }
        #messages {
            flex: 1;
            overflow-y: auto;
            padding: 1rem 2rem;
            display: flex;
            flex-direction: column;
            gap: 1rem;
        }
        .message {
            max-width: 80%;
            padding: 1rem;
            border-radius: 1rem;
            line-height: 1.5;
        }
        .message.user {
            align-self: flex-end;
            background: #0f3460;
            border-bottom-right-radius: 0.25rem;
        }
        .message.assistant {
            align-self: flex-start;
            background: #1a1a2e;
            border: 1px solid #0f3460;
            border-bottom-left-radius: 0.25rem;
        }
        .message pre {
            background: #0d1117;
            padding: 0.75rem;
            border-radius: 0.5rem;
            overflow-x: auto;
            margin: 0.5rem 0;
        }
        .message code { font-family: 'JetBrains Mono', monospace; font-size: 0.9rem; }
        .input-area {
            padding: 1rem 2rem;
            background: #16213e;
            border-top: 1px solid #0f3460;
        }
        .input-wrapper {
            display: flex;
            gap: 1rem;
            max-width: 1200px;
            margin: 0 auto;
        }
        #input {
            flex: 1;
            padding: 1rem;
            border: 1px solid #0f3460;
            border-radius: 0.5rem;
            background: #1a1a2e;
            color: #eee;
            font-size: 1rem;
            resize: none;
        }
        #input:focus { outline: none; border-color: #e94560; }
        button {
            padding: 1rem 2rem;
            background: #e94560;
            color: white;
            border: none;
            border-radius: 0.5rem;
            cursor: pointer;
            font-size: 1rem;
            font-weight: 600;
        }
        button:hover { background: #d63b54; }
        button:disabled { background: #666; cursor: not-allowed; }
        .tools-executed {
            font-size: 0.8rem;
            color: #888;
            margin-top: 0.5rem;
            padding-top: 0.5rem;
            border-top: 1px solid #333;
        }
        .tool-badge {
            display: inline-block;
            background: #0f3460;
            padding: 0.25rem 0.5rem;
            border-radius: 0.25rem;
            margin: 0.125rem;
            font-family: monospace;
        }
    </style>
</head>
<body>
    <header>
        <h1>🤖 OP-DBUS Chat</h1>
        <div class="status">
            <div class="status-dot" id="status-dot"></div>
            <span id="status-text">Connecting...</span>
            <span id="provider-info"></span>
        </div>
    </header>
    <main>
        <div id="messages"></div>
        <div class="input-area">
            <div class="input-wrapper">
                <textarea id="input" rows="2" placeholder="Ask me anything... (Enter to send)"></textarea>
                <button id="send">Send</button>
            </div>
        </div>
    </main>
    <script>
        const messagesEl = document.getElementById('messages');
        const inputEl = document.getElementById('input');
        const sendBtn = document.getElementById('send');
        const statusDot = document.getElementById('status-dot');
        const statusText = document.getElementById('status-text');
        const providerInfo = document.getElementById('provider-info');
        let sessionId = null;
        
        async function checkHealth() {
            try {
                const res = await fetch('/api/health');
                const data = await res.json();
                statusDot.classList.remove('error');
                statusText.textContent = 'Connected';
                return true;
            } catch (e) {
                statusDot.classList.add('error');
                statusText.textContent = 'Disconnected';
                return false;
            }
        }
        
        function addMessage(content, role, toolsExecuted = []) {
            const div = document.createElement('div');
            div.className = `message ${role}`;
            let html = content
                .replace(/```(\w*)\n([\s\S]*?)```/g, '<pre><code>$2</code></pre>')
                .replace(/`([^`]+)`/g, '<code>$1</code>')
                .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
                .replace(/\n/g, '<br>');
            div.innerHTML = html;
            if (toolsExecuted && toolsExecuted.length > 0) {
                const toolsDiv = document.createElement('div');
                toolsDiv.className = 'tools-executed';
                toolsDiv.innerHTML = '🔧 Tools: ' + toolsExecuted.map(t => `<span class="tool-badge">${t}</span>`).join('');
                div.appendChild(toolsDiv);
            }
            messagesEl.appendChild(div);
            messagesEl.scrollTop = messagesEl.scrollHeight;
        }
        
        async function sendMessage() {
            const message = inputEl.value.trim();
            if (!message) return;
            inputEl.value = '';
            sendBtn.disabled = true;
            addMessage(message, 'user');
            try {
                const res = await fetch('/api/chat', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ message, user_id: 'web-user', session_id: sessionId })
                });
                const data = await res.json();
                if (data.session_id) sessionId = data.session_id;
                if (data.provider) providerInfo.textContent = `| ${data.provider}/${data.model || 'default'}`;
                if (data.success) {
                    addMessage(data.message || 'No response', 'assistant', data.tools_executed);
                } else {
                    addMessage(`❌ Error: ${data.error || 'Unknown error'}`, 'assistant');
                }
            } catch (e) {
                addMessage(`❌ Network error: ${e.message}`, 'assistant');
            }
            sendBtn.disabled = false;
            inputEl.focus();
        }
        
        sendBtn.addEventListener('click', sendMessage);
        inputEl.addEventListener('keydown', (e) => {
            if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendMessage(); }
        });
        checkHealth();
        setInterval(checkHealth, 30000);
        addMessage('👋 Welcome to OP-DBUS Chat! How can I help you today?', 'assistant');
    </script>
</body>
</html>
HTML

echo "✅ Created $STATIC_DIR/index.html"

# Find the actual nginx config
NGINX_CONF=""
for conf in /etc/nginx/sites-available/op-dbus*.conf /etc/nginx/sites-available/op-dbus /etc/nginx/sites-available/op-web; do
    if [ -f "$conf" ]; then
        NGINX_CONF="$conf"
        break
    fi
done

if [ -z "$NGINX_CONF" ]; then
    NGINX_CONF="/etc/nginx/sites-available/op-dbus.conf"
fi

echo "📝 Creating nginx config: $NGINX_CONF"

# Find SSL cert
SSL_CERT=""
SSL_KEY=""
for cert_path in "/etc/letsencrypt/live/op-dbus.ghostbridge.tech/fullchain.pem" \
                 "/etc/letsencrypt/live/ghostbridge.tech/fullchain.pem" \
                 "/etc/nginx/ssl/ghostbridge.crt" \
                 "/etc/nginx/ssl/server.crt"; do
    if [ -f "$cert_path" ]; then
        SSL_CERT="$cert_path"
        SSL_KEY="${cert_path%fullchain.pem}privkey.pem"
        [ ! -f "$SSL_KEY" ] && SSL_KEY="${cert_path%.crt}.key"
        break
    fi
done

if [ -z "$SSL_CERT" ]; then
    echo "⚠️  No SSL cert found, using placeholder"
    SSL_CERT="/etc/nginx/ssl/server.crt"
    SSL_KEY="/etc/nginx/ssl/server.key"
fi

echo "   SSL cert: $SSL_CERT"
echo "   SSL key:  $SSL_KEY"

cat > "$NGINX_CONF" << NGINX
# op-dbus.ghostbridge.tech - Static frontend + API backend

upstream op_web_backend {
    server 127.0.0.1:8080;
    keepalive 32;
}

server {
    listen 80;
    listen [::]:80;
    server_name op-dbus.ghostbridge.tech;
    return 301 https://\$host\$request_uri;
}

server {
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name op-dbus.ghostbridge.tech;

    ssl_certificate $SSL_CERT;
    ssl_certificate_key $SSL_KEY;
    ssl_protocols TLSv1.2 TLSv1.3;

    access_log /var/log/nginx/op-dbus-access.log;
    error_log /var/log/nginx/op-dbus-error.log;

    # Serve static files at root
    root /var/www/op-dbus;
    index index.html;

    location / {
        try_files \$uri \$uri/ /index.html;
    }

    # API - proxy to backend
    location /api/ {
        proxy_pass http://op_web_backend/api/;
        proxy_http_version 1.1;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
    }

    # MCP - proxy to backend
    location /mcp/ {
        proxy_pass http://op_web_backend/mcp/;
        proxy_http_version 1.1;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 86400;
    }

    # Health check
    location = /health {
        proxy_pass http://op_web_backend/api/health;
    }
}
NGINX

# Enable site
ln -sf "$NGINX_CONF" /etc/nginx/sites-enabled/op-dbus.conf 2>/dev/null || true

# Test and reload
echo ""
echo "🔄 Testing nginx..."
if nginx -t 2>&1; then
    systemctl reload nginx
    echo "✅ Nginx reloaded"
else
    echo "❌ Nginx config error - check SSL paths"
    exit 1
fi

# Test
echo ""
echo "🧪 Testing..."
sleep 2
echo "  / -> $(curl -s -o /dev/null -w '%{http_code}' https://op-dbus.ghostbridge.tech/ 2>/dev/null || echo 'ERR')"
echo "  /api/health -> $(curl -s -o /dev/null -w '%{http_code}' https://op-dbus.ghostbridge.tech/api/health 2>/dev/null || echo 'ERR')"

echo ""
echo "🎉 Done! Open https://op-dbus.ghostbridge.tech/"
