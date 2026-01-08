#!/bin/bash
# deploy/deploy.sh
# Smart incremental deployment for op-dbus components.
# Detects changed files and only builds/deploys relevant services.
# Also handles new/updated systemd service files automatically.

set -e

# Configuration
INSTALL_DIR="/usr/local/sbin"
SYSTEMD_DIR="/etc/systemd/system"
PROJECT_ROOT=$(pwd)

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# State flags
BUILD_WEB=false
BUILD_LOGS=false
BUILD_MCP=false
BUILD_AGENTS=false
BUILD_DBUS=false

# Helper functions
log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_debug() { echo -e "${BLUE}[DEBUG]${NC} $1"; }

check_root() {
    if [[ $EUID -ne 0 ]]; then
        log_warn "This script requires sudo privileges for installation."
        sudo -v
    fi
}

# Generic deployment function for standard Rust components
deploy_component() {
    local package=$1
    local binary=$2
    local service=$3
    
    log_info "🚀 Deploying $package..."
    
    # 1. Build
    log_info "Building $package..."
    if cargo build --release -p "$package"; then
        log_info "Build successful."
    else
        log_error "Build failed for $package"
        return 1
    fi
    
    # 2. Stop service
    log_info "Stopping $service..."
    sudo systemctl stop "$service" || true
    
    # 3. Install binary
    local bin_path="target/release/$binary"
    if [[ -f "$bin_path" ]]; then
        log_info "Installing $binary to $INSTALL_DIR..."
        sudo cp "$bin_path" "$INSTALL_DIR/$binary"
        sudo chown root:root "$INSTALL_DIR/$binary"
        sudo chmod 755 "$INSTALL_DIR/$binary"
    else
        log_error "Binary not found at $bin_path"
        return 1
    fi

    # 3b. Create system directories and environment files
    sudo mkdir -p /etc/op-dbus /opt/op-dbus /var/lib/op-dbus /var/log/op-dbus
    sudo chown root:root /etc/op-dbus /opt/op-dbus /var/lib/op-dbus /var/log/op-dbus
    sudo chmod 755 /etc/op-dbus /opt/op-dbus /var/lib/op-dbus /var/log/op-dbus
    if [[ -f ".env" ]]; then
        sudo cp ".env" "/etc/op-dbus/environment"
        sudo chown root:root "/etc/op-dbus/environment"
        sudo chmod 644 "/etc/op-dbus/environment"
    fi
    
    # 4. Install/Update Service File
    # Always check if the service file in repo differs from installed one
    local repo_service="deploy/systemd/$service"
    local installed_service="$SYSTEMD_DIR/$service"

    if [[ -f "$repo_service" ]]; then
        # Check if installed file is different or missing
        if [[ ! -f "$installed_service" ]] || ! cmp -s "$repo_service" "$installed_service"; then
            log_info "Updating service file for $service..."
            sudo cp "$repo_service" "$SYSTEMD_DIR/"

            # Update paths in the installed service file to use system paths
            sudo sed -i "s|/home/jeremy/git/op-dbus-v2|/opt/op-dbus|g" "$installed_service"
            sudo sed -i "s|/home/jeremy/git/op-dbus-p1|/opt/op-dbus|g" "$installed_service"
            sudo sed -i "s|EnvironmentFile=/home/jeremy/git/op-dbus-v2/.env|EnvironmentFile=/etc/op-dbus/environment|g" "$installed_service"
            sudo sed -i "s|EnvironmentFile=/home/jeremy/git/op-dbus-p1/.env|EnvironmentFile=/etc/op-dbus/environment|g" "$installed_service"
            sudo sed -i "s|ExecStart=.*/target/release/|ExecStart=/usr/local/sbin/|g" "$installed_service"
            sudo sed -i "s|--bind 127.0.0.1:8082|--bind 0.0.0.0:8083|g" "$installed_service"
            sudo sed -i "s|--bind 0.0.0.0:8082|--bind 0.0.0.0:8083|g" "$installed_service"
            # Ensure services run as root
            sudo sed -i "s|User=jeremy|User=root|g" "$installed_service"
            sudo sed -i "s|Group=jeremy|Group=root|g" "$installed_service"
            # Fix systemd compatibility issues
            sudo sed -i "/StartLimitIntervalSec/d" "$installed_service"

            sudo systemctl daemon-reload
        fi
    fi
    
    # 5. Start service
    log_info "Starting $service..."
    sudo systemctl start "$service"
    
    # 6. Status check
    if systemctl is-active --quiet "$service"; then
        log_info "✅ $service is running."
    else
        log_error "❌ $service failed to start. Check logs: journalctl -u $service -n 20"
    fi
}

# Scan for any service files in deploy/systemd that aren't installed yet
install_new_services() {
    log_info "🔍 Scanning for new service files..."
    
    for service_file in deploy/systemd/*.service; do
        [ -e "$service_file" ] || continue
        
        local service_name=$(basename "$service_file")
        local installed_path="$SYSTEMD_DIR/$service_name"
        
        if [[ ! -f "$installed_path" ]]; then
            log_warn "Found new or missing service file: $service_name"
            log_info "Installing $service_name..."
            sudo cp "$service_file" "$installed_path"
            sudo systemctl daemon-reload
            
            # Enable and start, skipping templates
            if [[ "$service_name" != *"@.service" ]]; then
                log_info "Enabling and starting $service_name..."
                sudo systemctl enable --now "$service_name" || log_warn "Failed to start $service_name"
            fi
        else
            # Check if existing service needs update (for services NOT managed by deploy_component)
            # If it IS managed by deploy_component, it was handled above.
            # We can just blindly update if different? 
            # Risk: Restarting services that weren't built.
            # Safe approach: Only update if strictly new, or let the user handle "orphan" updates manually? 
            # User requirement: "updates any changed/new services".
            
            if ! cmp -s "$service_file" "$installed_path"; then
                # Check if this service is one of our main components. If so, skip (handled by deploy_component)
                if [[ "$service_name" != "op-web.service" && \
                      "$service_name" != "streaming-logs.service" && \
                      "$service_name" != "op-mcp.service" && \
                      "$service_name" != "op-agents.service" && \
                      "$service_name" != "op-dbus-service.service" ]]; then
                      
                    log_warn "Updating standalone service: $service_name"
                    sudo cp "$service_file" "$installed_path"
                    sudo systemctl daemon-reload
                    
                    # Skip restart for template units
                    if [[ "$service_name" != *"@.service" ]]; then
                        sudo systemctl restart "$service_name"
                    else
                        log_info "Skipping restart for template unit: $service_name"
                    fi
                fi
            fi
        fi
    done
}

# Deploy MCP configuration examples to /etc/mcp/
deploy_mcp_configs() {
    log_info "📋 Deploying MCP configuration examples to /etc/mcp/..."
    
    local mcp_config_dir="/etc/mcp"
    local example_dir="$PROJECT_ROOT/deploy/config/examples"
    
    if [[ ! -d "$example_dir" ]]; then
        log_warn "MCP config examples directory not found: $example_dir"
        return 0
    fi
    
    # Create target directory
    sudo mkdir -p "$mcp_config_dir"
    
    # Copy all JSON config files
    for config_file in "$example_dir"/*.json; do
        [ -e "$config_file" ] || continue
        local filename=$(basename "$config_file")
        local target="$mcp_config_dir/$filename"
        
        # Only copy if different or missing
        if [[ ! -f "$target" ]] || ! cmp -s "$config_file" "$target"; then
            log_info "Installing MCP config: $filename"
            sudo cp "$config_file" "$target"
            sudo chmod 644 "$target"
        fi
    done
    
    # Copy README
    if [[ -f "$example_dir/README.md" ]]; then
        sudo cp "$example_dir/README.md" "$mcp_config_dir/"
        sudo chmod 644 "$mcp_config_dir/README.md"
    fi
    
    log_info "✅ MCP configs deployed to $mcp_config_dir"
}

# --- Detection Logic ---

detect_changes() {
    log_info "🔍 Detecting changed components..."
    
    # Get list of changed files (staged and unstaged)
    local changed_files
    changed_files=$(git diff --name-only HEAD)
    
    if [[ -z "$changed_files" ]]; then
        log_warn "No changes detected in git tracked files."
        read -p "Force deploy all? [y/N] " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            # Still check for new services even if no code changed
            install_new_services
            exit 0
        fi
        BUILD_WEB=true; BUILD_LOGS=true; BUILD_MCP=true; BUILD_AGENTS=true; BUILD_DBUS=true
        return
    fi

    echo "$changed_files" | while read -r file; do
        if [[ -z "$file" ]]; then continue; fi
        
        # 1. Shared Libraries (Trigger multiple builds)
        if [[ $file == crates/op-core* ]]; then
            log_warn "Change in op-core detected. Rebuilding ALL."
            echo "ALL"
            return
        elif [[ $file == crates/op-tools* ]]; then
            echo "WEB"; echo "MCP"; echo "DBUS"; echo "AGENTS"
        elif [[ $file == crates/op-chat* || $file == crates/op-llm* || $file == crates/op-state* ]]; then
            echo "WEB"; echo "DBUS"; echo "MCP"
        
        # 2. Specific Components
        elif [[ $file == crates/op-web* || $file == deploy/systemd/op-web.service ]]; then
            echo "WEB"
        elif [[ $file == streaming-logs* || $file == deploy/systemd/streaming-logs.service ]]; then
            echo "LOGS"
        elif [[ $file == crates/op-mcp* || $file == deploy/systemd/op-mcp.service ]]; then
            echo "MCP"
        elif [[ $file == crates/op-agents* || $file == deploy/systemd/op-agents.service ]]; then
            echo "AGENTS"
        elif [[ $file == op-dbus-service* || $file == deploy/systemd/op-dbus-service.service ]]; then
            echo "DBUS"
        fi
    done | sort | uniq > /tmp/op_deploy_targets
    
    # Read targets from temp file
    if [[ -f /tmp/op_deploy_targets ]]; then
        while read -r target; do
            case $target in
                "ALL") BUILD_WEB=true; BUILD_LOGS=true; BUILD_MCP=true; BUILD_AGENTS=true; BUILD_DBUS=true ;; 
                "WEB") BUILD_WEB=true ;; 
                "LOGS") BUILD_LOGS=true ;; 
                "MCP") BUILD_MCP=true ;; 
                "AGENTS") BUILD_AGENTS=true ;; 
                "DBUS") BUILD_DBUS=true ;; 
            esac
        done < /tmp/op_deploy_targets
        rm /tmp/op_deploy_targets
    fi
}

# --- Main Execution ---

check_root

# Allow manual override
if [[ -n "$1" ]]; then
    case "$1" in
        "op-web"|"web") BUILD_WEB=true ;; 
        "logs"|"streaming-logs") BUILD_LOGS=true ;; 
        "mcp") BUILD_MCP=true ;; 
        "agents") BUILD_AGENTS=true ;; 
        "dbus") BUILD_DBUS=true ;; 
        "all") BUILD_WEB=true; BUILD_LOGS=true; BUILD_MCP=true; BUILD_AGENTS=true; BUILD_DBUS=true ;; 
        *) log_error "Unknown component: $1"; exit 1 ;; 
    esac
else
    detect_changes
fi

# Execute Builds
ANY_BUILT=false

if [ "$BUILD_WEB" = true ]; then
    deploy_component "op-web" "op-web-server" "op-web.service"
    ANY_BUILT=true
fi

if [ "$BUILD_LOGS" = true ]; then
    deploy_component "streaming-logs-admin" "streaming-logs-admin" "streaming-logs.service"
    ANY_BUILT=true
fi

if [ "$BUILD_MCP" = true ]; then
    deploy_component "op-mcp" "op-mcp-server" "op-mcp.service"
    ANY_BUILT=true
fi

if [ "$BUILD_AGENTS" = true ]; then
    deploy_component "op-agents" "dbus-agent" "op-agents.service"
    ANY_BUILT=true
fi

if [ "$BUILD_DBUS" = true ]; then
    deploy_component "op-dbus-service" "op-dbus-service" "op-dbus-service.service"
    ANY_BUILT=true
fi

# Check for new services at the end
install_new_services

# Deploy MCP configuration examples
deploy_mcp_configs

if [ "$ANY_BUILT" = false ]; then
    log_info "No main components deployed."
else
    log_info "🎉 Smart deployment complete!"
fi
