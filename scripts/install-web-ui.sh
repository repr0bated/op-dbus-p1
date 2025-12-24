#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UI_DIR="${ROOT_DIR}/crates/op-web-ui"

if [ ! -d "${UI_DIR}" ]; then
  echo "Missing UI directory: ${UI_DIR}" >&2
  exit 1
fi

if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "Installing wasm-pack..."
  cargo install wasm-pack
fi

if ! rustup target list --installed | grep -q '^wasm32-unknown-unknown$'; then
  rustup target add wasm32-unknown-unknown
fi

(cd "${UI_DIR}" && wasm-pack build --target web --release --out-dir pkg)

INSTALL_DIR="/usr/local/share/op-web-ui"
sudo mkdir -p "${INSTALL_DIR}"

if command -v rsync >/dev/null 2>&1; then
  sudo rsync -a --delete "${UI_DIR}/index.html" "${UI_DIR}/styles.css" "${UI_DIR}/pkg" "${INSTALL_DIR}/"
else
  sudo rm -rf "${INSTALL_DIR}/pkg"
  sudo cp -a "${UI_DIR}/pkg" "${INSTALL_DIR}/"
  sudo cp -a "${UI_DIR}/index.html" "${UI_DIR}/styles.css" "${INSTALL_DIR}/"
fi

ENV_FILE="/etc/op-dbus/environment"
sudo mkdir -p /etc/op-dbus
if sudo test -f "${ENV_FILE}"; then
  if sudo grep -q '^OP_WEB_STATIC_DIR=' "${ENV_FILE}"; then
    sudo sed -i 's|^OP_WEB_STATIC_DIR=.*|OP_WEB_STATIC_DIR=/usr/local/share/op-web-ui|' "${ENV_FILE}"
  else
    echo "OP_WEB_STATIC_DIR=/usr/local/share/op-web-ui" | sudo tee -a "${ENV_FILE}" >/dev/null
  fi
else
  echo "OP_WEB_STATIC_DIR=/usr/local/share/op-web-ui" | sudo tee "${ENV_FILE}" >/dev/null
fi

SERVICE_FILE="/etc/systemd/system/op-web.service"
if sudo test -f "${SERVICE_FILE}"; then
  if sudo grep -q '^Environment=OP_WEB_STATIC_DIR=' "${SERVICE_FILE}"; then
    sudo sed -i '/^Environment=OP_WEB_STATIC_DIR=/d' "${SERVICE_FILE}"
  fi
  sudo systemctl daemon-reload
  sudo systemctl restart op-web.service
fi

echo "Web UI installed to ${INSTALL_DIR}"
