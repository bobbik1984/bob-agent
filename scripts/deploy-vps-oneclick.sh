#!/usr/bin/env bash
set -euo pipefail

# ==============================================================================
# Bob VPS One-Click Deployment Script
# Idempotent cloud bootstrap for Debian/Ubuntu VPS
# ==============================================================================

echo "=== Bob Cloud Node Bootstrap ==="

# 1. Require root or sudo privileges
if [[ $EUID -ne 0 ]]; then
    if command -v sudo >/dev/null 2>&1; then
        exec sudo --preserve-env=BOB_PUBLIC_DOMAIN,BOB_BIND_ADDR,BOB_NODE_LABEL "$0" "$@"
    else
        echo "Error: This script must be run as root or with sudo." >&2
        exit 1
    fi
fi

# 2. Check & Install Docker / Docker Compose
if ! command -v docker >/dev/null 2>&1; then
    echo "[-] Docker not found. Installing Docker using official repository script..."
    apt-get update -y
    apt-get install -y curl ca-certificates
    curl -fsSL https://get.docker.com | sh
    systemctl enable --now docker
    echo "[+] Docker installed successfully."
fi

if ! docker compose version >/dev/null 2>&1; then
    echo "[-] Docker compose plugin not found. Installing docker-compose-plugin..."
    apt-get update -y
    apt-get install -y docker-compose-plugin
    echo "[+] Docker compose plugin installed."
fi

# 3. Determine Public Domain (environment variable or interactive prompt)
DOMAIN="${BOB_PUBLIC_DOMAIN:-}"
if [[ -z "$DOMAIN" ]]; then
    if [[ -t 0 ]]; then
        read -r -p "Enter public domain for Bob Node (e.g. bob.example.com): " DOMAIN
    fi
fi

if [[ -z "$DOMAIN" ]]; then
    DOMAIN="bob.example.com"
    echo "[!] No domain provided. Using default template domain: $DOMAIN"
fi

# 4. Determine script directory and repository root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# 5. Prepare private persistent directory (0700 per AEAD vault & Chromium isolation)
DATA_DIR="${BOB_DATA_DIR:-/var/lib/bob-node}"
echo "[*] Ensuring private data directory: ${DATA_DIR}"
mkdir -p "${DATA_DIR}"
chmod 700 "${DATA_DIR}"

# 6. Configure environment (.env file for docker compose)
ENV_FILE="${REPO_ROOT}/deploy/.env"
if [[ ! -f "$ENV_FILE" ]]; then
    echo "[*] Creating deploy/.env configuration..."
    cat > "$ENV_FILE" <<EOF
BOB_PUBLIC_DOMAIN=${DOMAIN}
BOB_BIND_ADDR=0.0.0.0:8787
BOB_NODE_LABEL=VPS-Cloud-Node
EOF
    chmod 600 "$ENV_FILE"
fi

# 7. Launch Container Stack via Docker Compose
echo "[*] Building and starting Bob Node services..."
cd "${REPO_ROOT}/deploy"
docker compose up -d --build

# 8. Print Pairing & Connection Instructions
echo ""
echo "=================================================================="
echo "  Bob Cloud Node Deployed Successfully!"
echo "=================================================================="
echo "  Public Web Entry: https://${DOMAIN}"
echo "  Direct API Port:  http://127.0.0.1:8787"
echo "  Data Directory:   ${DATA_DIR} (mode 0700)"
echo ""
echo "  To inspect startup logs and scan pairing QR code, run:"
echo "    docker compose -f ${REPO_ROOT}/deploy/docker-compose.yml logs -f bob-node"
echo "=================================================================="
