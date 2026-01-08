#!/bin/bash
#
# Pre-Implementation BTRFS Snapshot
#
# Creates a safety snapshot before major code changes.
# Run this BEFORE implementing new features.
#
# Usage:
#   sudo ./scripts/pre-impl-snapshot.sh [description]
#
# Examples:
#   sudo ./scripts/pre-impl-snapshot.sh "before grpc orchestration"
#   sudo ./scripts/pre-impl-snapshot.sh
#
# Rollback:
#   sudo ./scripts/pre-impl-snapshot.sh --rollback
#   sudo ./scripts/pre-impl-snapshot.sh --rollback <snapshot-name>
#

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

log_info()    { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn()    { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error()   { echo -e "${RED}[ERROR]${NC} $1"; }
log_step()    { echo -e "${CYAN}[STEP]${NC} $1"; }

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SNAPSHOT_BASE="/var/lib/op-dbus/snapshots/pre-impl"
SNAPSHOT_PREFIX="PRE-IMPL"
MAX_SNAPSHOTS=10
LOCK_FILE="/var/run/op-dbus-snapshot.lock"

# Detect BTRFS mount point containing project
detect_btrfs_mount() {
    local path="$1"
    
    # Get the mount point and filesystem type
    local mount_info
    mount_info=$(df --output=target,fstype "$path" 2>/dev/null | tail -1)
    
    local mount_point fs_type
    mount_point=$(echo "$mount_info" | awk '{print $1}')
    fs_type=$(echo "$mount_info" | awk '{print $2}')
    
    if [[ "$fs_type" != "btrfs" ]]; then
        return 1
    fi
    
    echo "$mount_point"
    return 0
}

# Get BTRFS device for a path
get_btrfs_device() {
    local path="$1"
    findmnt -n -o SOURCE -T "$path" 2>/dev/null | head -1 | sed 's/\[.*\]//'
}

# Check if path is a BTRFS subvolume
is_subvolume() {
    local path="$1"
    btrfs subvolume show "$path" &>/dev/null
}

# Create snapshot directory structure
ensure_snapshot_dir() {
    if [[ ! -d "$SNAPSHOT_BASE" ]]; then
        log_info "Creating snapshot directory: $SNAPSHOT_BASE"
        mkdir -p "$SNAPSHOT_BASE"
    fi
    
    # Check if snapshot base is on BTRFS
    local fs_type
    fs_type=$(df --output=fstype "$SNAPSHOT_BASE" 2>/dev/null | tail -1 | tr -d ' ')
    
    if [[ "$fs_type" != "btrfs" ]]; then
        log_warn "Snapshot directory is not on BTRFS filesystem"
        log_warn "Snapshots will be regular directory copies (slower)"
        return 1
    fi
    
    return 0
}

# Generate snapshot name
generate_snapshot_name() {
    local description="${1:-}"
    local timestamp
    timestamp=$(date +%Y%m%d-%H%M%S)
    
    # Count existing snapshots to get next number
    local count
    count=$(find "$SNAPSHOT_BASE" -maxdepth 1 -name "${SNAPSHOT_PREFIX}-*" -type d 2>/dev/null | wc -l)
    local next_num=$((count + 1))
    
    if [[ -n "$description" ]]; then
        # Sanitize description (remove special chars, limit length)
        local safe_desc
        safe_desc=$(echo "$description" | tr -cs 'a-zA-Z0-9' '-' | head -c 30 | sed 's/-$//')
        echo "${SNAPSHOT_PREFIX}-${timestamp}-${safe_desc}"
    else
        echo "${SNAPSHOT_PREFIX}-${timestamp}"
    fi
}

# Create BTRFS snapshot
create_btrfs_snapshot() {
    local source="$1"
    local dest="$2"
    
    log_step "Creating BTRFS snapshot: $dest"
    
    if btrfs subvolume snapshot -r "$source" "$dest" 2>/dev/null; then
        log_success "Created read-only BTRFS snapshot"
        return 0
    else
        log_warn "BTRFS snapshot failed, falling back to rsync"
        return 1
    fi
}

# Fallback: Create directory copy with rsync
create_rsync_snapshot() {
    local source="$1"
    local dest="$2"
    
    log_step "Creating rsync snapshot: $dest"
    
    mkdir -p "$dest"
    
    # Use rsync with hard links for efficiency
    if rsync -a --link-dest="$source" "$source/" "$dest/" 2>/dev/null; then
        log_success "Created rsync snapshot with hard links"
        return 0
    else
        log_error "Rsync snapshot failed"
        return 1
    fi
}

# Prune old snapshots
prune_old_snapshots() {
    local snapshots
    snapshots=$(find "$SNAPSHOT_BASE" -maxdepth 1 -name "${SNAPSHOT_PREFIX}-*" -type d 2>/dev/null | sort)
    local count
    count=$(echo "$snapshots" | grep -c . || echo 0)
    
    if [[ $count -le $MAX_SNAPSHOTS ]]; then
        log_info "Snapshot count ($count) within limit ($MAX_SNAPSHOTS)"
        return 0
    fi
    
    local to_delete=$((count - MAX_SNAPSHOTS))
    log_info "Pruning $to_delete old snapshots (keeping $MAX_SNAPSHOTS)"
    
    echo "$snapshots" | head -n "$to_delete" | while read -r snapshot; do
        if [[ -z "$snapshot" ]]; then
            continue
        fi
        
        log_step "Deleting old snapshot: $(basename "$snapshot")"
        
        # Try BTRFS delete first
        if btrfs subvolume delete "$snapshot" 2>/dev/null; then
            log_success "Deleted BTRFS subvolume"
        else
            # Fallback to rm
            rm -rf "$snapshot"
            log_success "Deleted directory"
        fi
    done
}

# List available snapshots
list_snapshots() {
    echo ""
    echo "Available pre-implementation snapshots:"
    echo "========================================"
    
    if [[ ! -d "$SNAPSHOT_BASE" ]]; then
        echo "  (no snapshots found)"
        return
    fi
    
    local snapshots
    snapshots=$(find "$SNAPSHOT_BASE" -maxdepth 1 -name "${SNAPSHOT_PREFIX}-*" -type d 2>/dev/null | sort -r)
    
    if [[ -z "$snapshots" ]]; then
        echo "  (no snapshots found)"
        return
    fi
    
    local idx=1
    echo "$snapshots" | while read -r snapshot; do
        if [[ -z "$snapshot" ]]; then
            continue
        fi
        
        local name
        name=$(basename "$snapshot")
        
        # Get creation time
        local created
        created=$(stat -c %y "$snapshot" 2>/dev/null | cut -d. -f1)
        
        # Get size
        local size
        size=$(du -sh "$snapshot" 2>/dev/null | cut -f1)
        
        # Check if BTRFS subvolume
        local type="dir"
        if btrfs subvolume show "$snapshot" &>/dev/null; then
            type="btrfs"
        fi
        
        printf "  %2d. %-50s [%s] %s (%s)\n" "$idx" "$name" "$type" "$size" "$created"
        idx=$((idx + 1))
    done
    
    echo ""
}

# Rollback to a snapshot
rollback_to_snapshot() {
    local snapshot_name="${1:-}"
    
    if [[ -z "$snapshot_name" ]]; then
        # Find most recent snapshot
        snapshot_name=$(find "$SNAPSHOT_BASE" -maxdepth 1 -name "${SNAPSHOT_PREFIX}-*" -type d 2>/dev/null | sort -r | head -1)
        
        if [[ -z "$snapshot_name" ]]; then
            log_error "No snapshots found to rollback to"
            exit 1
        fi
        
        snapshot_name=$(basename "$snapshot_name")
    fi
    
    local snapshot_path="$SNAPSHOT_BASE/$snapshot_name"
    
    if [[ ! -d "$snapshot_path" ]]; then
        log_error "Snapshot not found: $snapshot_name"
        list_snapshots
        exit 1
    fi
    
    echo ""
    log_warn "ROLLBACK WARNING"
    echo "================="
    echo "This will restore the project to snapshot: $snapshot_name"
    echo "Current changes will be LOST unless you create a new snapshot first."
    echo ""
    echo "Snapshot path: $snapshot_path"
    echo "Target path:   $PROJECT_ROOT"
    echo ""
    
    read -p "Create a backup snapshot before rollback? [Y/n] " create_backup
    if [[ "$create_backup" != "n" && "$create_backup" != "N" ]]; then
        log_info "Creating backup snapshot before rollback..."
        create_snapshot "pre-rollback-backup"
    fi
    
    read -p "Proceed with rollback? [y/N] " confirm
    if [[ "$confirm" != "y" && "$confirm" != "Y" ]]; then
        log_info "Rollback cancelled"
        exit 0
    fi
    
    log_step "Rolling back to: $snapshot_name"
    
    # Use rsync to restore (preserves current .git if needed)
    rsync -a --delete \
        --exclude='.git' \
        --exclude='target' \
        --exclude='node_modules' \
        "$snapshot_path/" "$PROJECT_ROOT/"
    
    log_success "Rollback complete!"
    log_info "Run 'cargo build' to rebuild after rollback"
}

# Create a new snapshot
create_snapshot() {
    local description="${1:-}"
    
    # Acquire lock
    exec 200>"$LOCK_FILE"
    if ! flock -n 200; then
        log_error "Another snapshot operation is in progress"
        exit 1
    fi
    
    echo ""
    log_info "Creating pre-implementation snapshot"
    echo "======================================"
    echo "Project root: $PROJECT_ROOT"
    echo "Description:  ${description:-<none>}"
    echo ""
    
    # Ensure snapshot directory exists
    local use_btrfs=true
    if ! ensure_snapshot_dir; then
        use_btrfs=false
    fi
    
    # Generate snapshot name
    local snapshot_name
    snapshot_name=$(generate_snapshot_name "$description")
    local snapshot_path="$SNAPSHOT_BASE/$snapshot_name"
    
    log_info "Snapshot name: $snapshot_name"
    
    # Check if project is on BTRFS and is a subvolume
    local project_is_btrfs=false
    if is_subvolume "$PROJECT_ROOT" && $use_btrfs; then
        project_is_btrfs=true
        log_info "Project is a BTRFS subvolume - using native snapshot"
    fi
    
    # Create snapshot
    local snapshot_created=false
    
    if $project_is_btrfs; then
        if create_btrfs_snapshot "$PROJECT_ROOT" "$snapshot_path"; then
            snapshot_created=true
        fi
    fi
    
    if ! $snapshot_created; then
        if create_rsync_snapshot "$PROJECT_ROOT" "$snapshot_path"; then
            snapshot_created=true
        fi
    fi
    
    if ! $snapshot_created; then
        log_error "Failed to create snapshot"
        exit 1
    fi
    
    # Write metadata
    cat > "$snapshot_path/.snapshot-meta.json" << EOF
{
    "name": "$snapshot_name",
    "created": "$(date -Iseconds)",
    "description": "$description",
    "project_root": "$PROJECT_ROOT",
    "git_commit": "$(cd "$PROJECT_ROOT" && git rev-parse HEAD 2>/dev/null || echo 'unknown')",
    "git_branch": "$(cd "$PROJECT_ROOT" && git rev-parse --abbrev-ref HEAD 2>/dev/null || echo 'unknown')",
    "git_dirty": $(cd "$PROJECT_ROOT" && git diff --quiet 2>/dev/null && echo 'false' || echo 'true'),
    "user": "$(whoami)",
    "hostname": "$(hostname)"
}
EOF
    
    # Prune old snapshots
    prune_old_snapshots
    
    echo ""
    log_success "Snapshot created successfully!"
    echo ""
    echo "Snapshot: $snapshot_name"
    echo "Path:     $snapshot_path"
    echo ""
    echo "To rollback to this snapshot:"
    echo "  sudo $0 --rollback $snapshot_name"
    echo ""
    echo "To list all snapshots:"
    echo "  sudo $0 --list"
    echo ""
    
    # Release lock
    flock -u 200
}

# Show help
show_help() {
    echo "Usage: $0 [OPTIONS] [description]"
    echo ""
    echo "Create a BTRFS snapshot before implementing major changes."
    echo ""
    echo "Options:"
    echo "  --rollback [name]   Rollback to a snapshot (latest if no name given)"
    echo "  --list              List available snapshots"
    echo "  --prune             Prune old snapshots (keep last $MAX_SNAPSHOTS)"
    echo "  --help              Show this help"
    echo ""
    echo "Examples:"
    echo "  sudo $0                              # Create snapshot with auto name"
    echo "  sudo $0 \"before grpc impl\"          # Create snapshot with description"
    echo "  sudo $0 --rollback                   # Rollback to latest snapshot"
    echo "  sudo $0 --rollback PRE-IMPL-20250115 # Rollback to specific snapshot"
    echo ""
}

# Main
main() {
    # Check root
    if [[ $EUID -ne 0 ]]; then
        log_error "This script must be run as root (for BTRFS operations)"
        echo "Run: sudo $0 $*"
        exit 1
    fi
    
    # Parse arguments
    case "${1:-}" in
        --rollback)
            rollback_to_snapshot "${2:-}"
            ;;
        --list)
            list_snapshots
            ;;
        --prune)
            prune_old_snapshots
            ;;
        --help|-h)
            show_help
            ;;
        --*)
            log_error "Unknown option: $1"
            show_help
            exit 1
            ;;
        *)
            create_snapshot "$*"
            ;;
    esac
}

main "$@"
