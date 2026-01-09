#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  install-antigravity-bridge.sh --upstream URL [options]

Options:
  --upstream URL     Upstream OpenAI-compatible /v1/chat/completions endpoint (required)
  --port PORT        Local listen port (default: 3333)
  --auth VALUE       Optional Authorization header value (e.g. "Bearer <token>")
  --env-file PATH    Bridge env file (default: /etc/antigravity-bridge.env)
  --install-dir DIR  Install directory (default: /opt/antigravity-bridge)
  --service NAME     Systemd service name (default: antigravity-bridge)
  --configure-opdbus Update /etc/op-dbus/environment if present
  --model MODEL      Set LLM_MODEL in /etc/op-dbus/environment (optional)
  --skip-globalpython  Skip global Python/uv setup
  --globalpython-dir DIR  Global Python venv path (default: /opt/globalpython)
  --snapshot-dir DIR      btrfs snapshot base (default: /.snapshots/uv-projects)
  --build-vsix      Build Antigravity IDE extension VSIX
  --vsix-out PATH   VSIX output path (default: extensions/antigravity-bridge/op-dbus-antigravity-bridge.vsix)

Examples:
  sudo ./install-antigravity-bridge.sh \
    --upstream http://127.0.0.1:7788/v1/chat/completions \
    --configure-opdbus
EOF
}

UPSTREAM_URL=""
PORT="3333"
AUTH_VALUE=""
ENV_FILE="/etc/antigravity-bridge.env"
INSTALL_DIR="/opt/antigravity-bridge"
SERVICE_NAME="antigravity-bridge"
CONFIGURE_OPDBUS="false"
MODEL=""
SETUP_GLOBALPYTHON="true"
GLOBALPYTHON_DIR="/opt/globalpython"
SNAPSHOT_DIR="/.snapshots/uv-projects"
BUILD_VSIX="false"
VSIX_OUT=""

while [ $# -gt 0 ]; do
  case "$1" in
    --upstream)
      UPSTREAM_URL="${2:-}"
      shift 2
      ;;
    --port)
      PORT="${2:-}"
      shift 2
      ;;
    --auth)
      AUTH_VALUE="${2:-}"
      shift 2
      ;;
    --env-file)
      ENV_FILE="${2:-}"
      shift 2
      ;;
    --install-dir)
      INSTALL_DIR="${2:-}"
      shift 2
      ;;
    --service)
      SERVICE_NAME="${2:-}"
      shift 2
      ;;
    --configure-opdbus)
      CONFIGURE_OPDBUS="true"
      shift 1
      ;;
    --model)
      MODEL="${2:-}"
      shift 2
      ;;
    --skip-globalpython)
      SETUP_GLOBALPYTHON="false"
      shift 1
      ;;
    --globalpython-dir)
      GLOBALPYTHON_DIR="${2:-}"
      shift 2
      ;;
    --snapshot-dir)
      SNAPSHOT_DIR="${2:-}"
      shift 2
      ;;
    --build-vsix)
      BUILD_VSIX="true"
      shift 1
      ;;
    --vsix-out)
      VSIX_OUT="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      usage
      exit 1
      ;;
  esac
done

if [ -z "$UPSTREAM_URL" ]; then
  echo "Error: --upstream is required."
  usage
  exit 1
fi

if ! command -v node >/dev/null 2>&1; then
  echo "Error: node is required but not found in PATH."
  exit 1
fi

NODE_BIN="$(command -v node)"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC_BRIDGE="$SCRIPT_DIR/antigravity-bridge.js"

if [ ! -f "$SRC_BRIDGE" ]; then
  echo "Error: bridge script not found at $SRC_BRIDGE"
  exit 1
fi

install_uv() {
  if command -v uv >/dev/null 2>&1; then
    return 0
  fi

  if ! command -v curl >/dev/null 2>&1; then
    echo "Error: curl is required to install uv."
    exit 1
  fi

  echo "Installing uv..."
  local arch
  arch="$(uname -m)"
  local uv_pkg
  case "$arch" in
    x86_64)
      uv_pkg="uv-x86_64-unknown-linux-gnu.tar.gz"
      ;;
    aarch64|arm64)
      uv_pkg="uv-aarch64-unknown-linux-gnu.tar.gz"
      ;;
    *)
      echo "Unsupported architecture for uv install: $arch"
      echo "Install uv manually, then re-run this installer."
      exit 1
      ;;
  esac

  local tmp_dir
  tmp_dir="$(mktemp -d)"
  curl -fsSL "https://github.com/astral-sh/uv/releases/latest/download/${uv_pkg}" -o "${tmp_dir}/${uv_pkg}"
  tar -xzf "${tmp_dir}/${uv_pkg}" -C "${tmp_dir}"
  sudo install -m 0755 "${tmp_dir}/uv" /usr/local/bin/uv
  if [ -f "${tmp_dir}/uvx" ]; then
    sudo install -m 0755 "${tmp_dir}/uvx" /usr/local/bin/uvx
  fi
  rm -rf "$tmp_dir"
}

setup_globalpython() {
  echo "Setting up global Python venv at ${GLOBALPYTHON_DIR}..."
  install_uv

  if [ ! -d "$GLOBALPYTHON_DIR" ]; then
    sudo /usr/local/bin/uv venv "$GLOBALPYTHON_DIR"
  fi

  sudo install -d /usr/local/bin
  sudo tee /usr/local/bin/uv-project-snapshot >/dev/null <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

SNAPSHOT_BASE="${UV_SNAPSHOT_DIR:-/.snapshots/uv-projects}"
PROJECT_FILE="${UV_PROJECT_FILE:-pyproject.toml}"

if [ ! -f "$PROJECT_FILE" ]; then
  exit 0
fi

project_dir="$(pwd)"
project_name="$(basename "$project_dir")"
cache_dir="${HOME}/.cache/uv-snapshots"
cache_file="${cache_dir}/${project_name}.sha"

mkdir -p "$cache_dir"

current_hash="$(sha256sum "$PROJECT_FILE" | awk '{print $1}')"
previous_hash=""
if [ -f "$cache_file" ]; then
  previous_hash="$(cat "$cache_file")"
fi

if [ "$current_hash" = "$previous_hash" ]; then
  exit 0
fi

timestamp="$(date +%Y%m%d-%H%M%S)"
snapshot_dir="${SNAPSHOT_BASE}/${project_name}/${timestamp}"

mkdir -p "$snapshot_dir"
cp "$PROJECT_FILE" "${snapshot_dir}/"
echo "$current_hash" > "$cache_file"
EOF
  sudo chmod +x /usr/local/bin/uv-project-snapshot

  sudo tee /etc/profile.d/uv-env.sh >/dev/null <<EOF
export UV_GLOBAL_VENV="${GLOBALPYTHON_DIR}"
export UV_SNAPSHOT_DIR="${SNAPSHOT_DIR}"

_uv_prompt_prefix() {
  if [ -n "\${VIRTUAL_ENV-}" ]; then
    printf '[venv:%s] ' "\$(basename "\$VIRTUAL_ENV")"
  fi
}

if [ -n "\${BASH_VERSION-}" ]; then
  case "\${PROMPT_COMMAND-}" in
    *"_uv_prompt_prefix"*) ;;
    *) PROMPT_COMMAND="_uv_prompt_prefix; \${PROMPT_COMMAND:-:}" ;;
  esac
  case "\${PS1-}" in
    *"\\\$( _uv_prompt_prefix )"*) ;;
    *) PS1='\\\$( _uv_prompt_prefix )'"$PS1" ;;
  esac
fi

pip() { uv pip "\$@"; }
venv() { uv venv "\$@"; }

if [ -z "\${UV_SNAPSHOT_HOOKED-}" ]; then
  export UV_SNAPSHOT_HOOKED=1
  if [ -n "\${BASH_VERSION-}" ]; then
    PROMPT_COMMAND="uv-project-snapshot >/dev/null 2>&1; \${PROMPT_COMMAND:-:}"
  fi
fi
EOF
}

build_vsix() {
  local ext_dir="${SCRIPT_DIR}/../extensions/antigravity-bridge"
  if [ ! -d "$ext_dir" ]; then
    echo "Extension directory not found: $ext_dir"
    exit 1
  fi
  if ! command -v npm >/dev/null 2>&1; then
    echo "Error: npm is required to build the VSIX."
    exit 1
  fi
  local output_path="$VSIX_OUT"
  if [ -z "$output_path" ]; then
    output_path="${ext_dir}/op-dbus-antigravity-bridge.vsix"
  fi
  echo "Building VSIX..."
  (cd "$ext_dir" && npm install && npm run compile && npx @vscode/vsce package --out "$output_path")
  echo "VSIX built at: $output_path"
}

echo "Installing Antigravity bridge..."
sudo mkdir -p "$INSTALL_DIR"
sudo install -m 0755 "$SRC_BRIDGE" "$INSTALL_DIR/antigravity-bridge.js"

echo "Writing env file: $ENV_FILE"
sudo tee "$ENV_FILE" >/dev/null <<EOF
ANTIGRAVITY_BRIDGE_PORT=$PORT
ANTIGRAVITY_UPSTREAM_URL=$UPSTREAM_URL
EOF

if [ -n "$AUTH_VALUE" ]; then
  sudo sh -c "echo \"ANTIGRAVITY_UPSTREAM_AUTH=$AUTH_VALUE\" >> \"$ENV_FILE\""
fi

SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}.service"
echo "Writing systemd service: $SERVICE_FILE"
sudo tee "$SERVICE_FILE" >/dev/null <<EOF
[Unit]
Description=Antigravity Bridge (OpenAI-compatible proxy)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
Group=root
EnvironmentFile=$ENV_FILE
WorkingDirectory=$INSTALL_DIR
ExecStart=$NODE_BIN $INSTALL_DIR/antigravity-bridge.js
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
EOF

set_env_var() {
  local file="$1"
  local key="$2"
  local value="$3"

  if sudo grep -q "^${key}=" "$file"; then
    sudo sed -i "s|^${key}=.*|${key}=${value}|" "$file"
  else
    echo "${key}=${value}" | sudo tee -a "$file" >/dev/null
  fi
}

if [ "$CONFIGURE_OPDBUS" = "true" ] && [ -f /etc/op-dbus/environment ]; then
  echo "Updating /etc/op-dbus/environment for antigravity bridge..."
  set_env_var /etc/op-dbus/environment LLM_PROVIDER antigravity
  set_env_var /etc/op-dbus/environment ANTIGRAVITY_BRIDGE_URL "http://127.0.0.1:${PORT}"
  if [ -n "$MODEL" ]; then
    set_env_var /etc/op-dbus/environment LLM_MODEL "$MODEL"
  fi
fi

if [ "$SETUP_GLOBALPYTHON" = "true" ]; then
  setup_globalpython
fi

if [ "$BUILD_VSIX" = "true" ]; then
  build_vsix
fi

echo "Enabling and starting ${SERVICE_NAME}..."
sudo systemctl daemon-reload
sudo systemctl enable --now "$SERVICE_NAME"

echo "Done."
echo "Health check: curl http://127.0.0.1:${PORT}/health"
