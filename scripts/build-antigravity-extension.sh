#!/bin/bash
# Build and package the Antigravity Bridge extension

set -e

EXT_DIR="extensions/antigravity-bridge"

if [ ! -d "$EXT_DIR" ]; then
    echo "❌ Extension directory not found: $EXT_DIR"
    echo "   Run from the repository root"
    exit 1
fi

cd "$EXT_DIR"

echo "📦 Installing dependencies..."
npm install

echo "🔨 Compiling TypeScript..."
npm run compile

echo "📦 Packaging extension..."
# Install vsce if not present
if ! command -v vsce &> /dev/null; then
    echo "   Installing vsce..."
    npm install -g @vscode/vsce
fi

vsce package --allow-missing-repository || {
    echo "⚠️  vsce package failed, creating manual package..."
    # Manual packaging fallback
    mkdir -p dist
    cp -r out package.json dist/
    cd dist && zip -r ../antigravity-bridge.vsix . && cd ..
    rm -rf dist
}

VSIX=$(ls -1 *.vsix 2>/dev/null | head -1)

if [ -n "$VSIX" ]; then
    echo ""
    echo "✅ Extension built: $EXT_DIR/$VSIX"
    echo ""
    echo "📋 To install in Antigravity IDE:"
    echo "   1. Open Antigravity"
    echo "   2. Ctrl+Shift+P → 'Extensions: Install from VSIX'"
    echo "   3. Select: $(pwd)/$VSIX"
    echo ""
    echo "📋 Then in op-dbus:"
    echo "   export ANTIGRAVITY_ENABLED=true"
    echo "   export ANTIGRAVITY_BRIDGE_URL=http://127.0.0.1:7788"
    echo "   export LLM_PROVIDER=antigravity"
else
    echo "❌ Failed to build extension"
    exit 1
fi
