#!/bin/bash
#
# Check for forbidden dependencies in op-dbus
#
# Forbidden:
#   - nix crate (use libc directly)
#   - pocketflow (native workflow engine)
#   - any node.js dependencies
#

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_ok() { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_err() { echo -e "${RED}[ERROR]${NC} $1"; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

echo "Checking for forbidden dependencies..."
echo ""

ERRORS=0

# Check for nix crate in Cargo.toml (excluding comments)
echo "Checking for 'nix' crate..."
NIX_DEPS=$(grep -r '^[^#]*nix\s*=' --include="Cargo.toml" . 2>/dev/null || true)
if [[ -n "$NIX_DEPS" ]]; then
    log_err "Found nix crate dependency (FORBIDDEN):"
    echo "$NIX_DEPS" | sed 's/^/    /'
    ERRORS=$((ERRORS + 1))
else
    log_ok "No nix crate dependencies"
fi

# Check for nix imports in Rust code
echo "Checking for 'use nix::' imports..."
NIX_IMPORTS=$(grep -r 'use nix::' --include="*.rs" crates/ 2>/dev/null || true)
if [[ -n "$NIX_IMPORTS" ]]; then
    log_err "Found nix imports (FORBIDDEN):"
    echo "$NIX_IMPORTS" | sed 's/^/    /'
    ERRORS=$((ERRORS + 1))
else
    log_ok "No nix imports"
fi

# Check for pocketflow
echo "Checking for 'pocketflow' dependency..."
POCKETFLOW=$(grep -r 'pocketflow' --include="Cargo.toml" --include="*.rs" crates/ 2>/dev/null || true)
if [[ -n "$POCKETFLOW" ]]; then
    log_err "Found pocketflow references (FORBIDDEN):"
    echo "$POCKETFLOW" | sed 's/^/    /'
    ERRORS=$((ERRORS + 1))
else
    log_ok "No pocketflow references"
fi

# Check for node.js artifacts
echo "Checking for Node.js artifacts..."
NODE_ARTIFACTS=$(find . -name "package.json" -o -name "node_modules" -o -name "yarn.lock" -o -name "package-lock.json" 2>/dev/null | grep -v target || true)
if [[ -n "$NODE_ARTIFACTS" ]]; then
    log_warn "Found Node.js artifacts (should be removed for pure Rust):"
    echo "$NODE_ARTIFACTS" | sed 's/^/    /'
else
    log_ok "No Node.js artifacts"
fi

# Check for Python artifacts (except venv which is OK for tooling)
echo "Checking for Python artifacts..."
PYTHON_ARTIFACTS=$(find . -name "requirements.txt" -o -name "setup.py" -o -name "pyproject.toml" 2>/dev/null | grep -v target | grep -v .venv || true)
if [[ -n "$PYTHON_ARTIFACTS" ]]; then
    log_warn "Found Python artifacts:"
    echo "$PYTHON_ARTIFACTS" | sed 's/^/    /'
else
    log_ok "No Python project files"
fi

echo ""
if [[ $ERRORS -gt 0 ]]; then
    log_err "Found $ERRORS forbidden dependency issues!"
    exit 1
else
    log_ok "All dependency checks passed"
fi
