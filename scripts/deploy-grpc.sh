#!/bin/bash
#
# Deploy gRPC MCP Server
#

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
INSTALL_DIR="/usr/local/sbin"
SYSTEMD_DIR="/etc/systemd/system"
DATA_DIR="/var/lib/op-dbus"

GRPC_PORT="50051"
GRPC_AGENTS_PORT="50052"

cd "$PROJECT_ROOT"

log_info "Checking gRPC module..."
if [[ ! -d "crates/op-mcp/src/grpc" ]]; then
    log_error "gRPC module not found at crates/op-mcp/src/grpc"
    exit 1
fi

if [[ ! -f "crates/op-mcp/proto/mcp.proto" ]]; then
    log_error "Proto file not found at crates/op-mcp/proto/mcp.proto"
    exit 1
fi
log_success "gRPC module found"

log_info "Building MCP server with gRPC feature..."
if cargo build --release -p op-mcp --features grpc; then
    log_success "Build complete"
else
    log_error "Build failed"
    exit 1
fi

if [[ ! -f "target/release/op-mcp-server" ]]; then
    log_error "Binary not found: target/release/op-mcp-server"
    exit 1
fi

if [[ $EUID -ne 0 ]]; then
    log_info "Sudo required for installation..."
    sudo -v || { log_error "Sudo authentication failed"; exit 1; }
fi

log_info "Creating infrastructure directories..."
sudo mkdir -p "$DATA_DIR"/{cache/grpc,state,blockchain/grpc}
log_success "Directories created"

log_info "Stopping existing gRPC services..."
sudo systemctl stop op-mcp-grpc 2>/dev/null || true
sudo systemctl stop op-mcp-grpc-agents 2>/dev/null || true

log_info "Installing binary..."
sudo cp target/release/op-mcp-server "$INSTALL_DIR/"
sudo chmod 755 "$INSTALL_DIR/op-mcp-server"
log_success "Installed: $INSTALL_DIR/op-mcp-server"

log_info "Installing systemd services..."
if [[ -f "$PROJECT_ROOT/deploy/systemd/op-mcp-grpc.service" ]]; then
    sudo cp "$PROJECT_ROOT/deploy/systemd/op-mcp-grpc.service" "$SYSTEMD_DIR/"
    log_success "Installed op-mcp-grpc.service"
fi

if [[ -f "$PROJECT_ROOT/deploy/systemd/op-mcp-grpc-agents.service" ]]; then
    sudo cp "$PROJECT_ROOT/deploy/systemd/op-mcp-grpc-agents.service" "$SYSTEMD_DIR/"
    log_success "Installed op-mcp-grpc-agents.service"
fi

log_info "Starting services..."
sudo systemctl daemon-reload
sudo systemctl enable op-mcp-grpc op-mcp-grpc-agents 2>/dev/null || true
sudo systemctl start op-mcp-grpc op-mcp-grpc-agents

sleep 2

echo ""
log_info "Service Status:"
for svc in op-mcp-grpc op-mcp-grpc-agents; do
    if systemctl is-active --quiet "$svc"; then
        echo -e "  $svc: ${GREEN}running${NC}"
    else
        echo -e "  $svc: ${RED}failed${NC}"
        journalctl -u "$svc" -n 3 --no-pager 2>/dev/null | sed 's/^/    /'
    fi
done

echo ""
log_info "gRPC Endpoints:"
echo -e "  Compact:  ${YELLOW}grpc://localhost:$GRPC_PORT${NC}"
echo -e "  Agents:   ${YELLOW}grpc://localhost:$GRPC_AGENTS_PORT${NC}"

echo ""
log_success "gRPC deployment complete!"
