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
        exec sudo --preserve-env=BOB_PUBLIC_DOMAIN,BOB_BIND_ADDR,BOB_NODE_LABEL,BOB_DATA_DIR "$0" "$@"
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

# 5. Preflight CJK fonts & prepare private persistent directories (0700 per AEAD vault & Chromium isolation)
DATA_DIR="${BOB_DATA_DIR:-/var/lib/bob-node}"
BROWSER_PROFILE_DIR="${DATA_DIR}/browser_profile"
echo "[*] Ensuring private data and Chromium profile directories (mode 0700): ${DATA_DIR}"
mkdir -p "${DATA_DIR}" "${BROWSER_PROFILE_DIR}"
chmod 700 "${DATA_DIR}" "${BROWSER_PROFILE_DIR}"

echo "[*] Checking CJK font availability for headless Chromium rendering..."
HAS_CJK_FONTS=0
if command -v fc-list >/dev/null 2>&1; then
    if [[ -n "$(fc-list :lang=zh 2>/dev/null || true)" ]]; then
        HAS_CJK_FONTS=1
    fi
fi

if [[ "$HAS_CJK_FONTS" -eq 0 ]]; then
    if command -v apt-get >/dev/null 2>&1; then
        echo "[-] Host CJK fonts missing. Installing fonts-noto-cjk and fontconfig..."
        DEBIAN_FRONTEND=noninteractive apt-get update -y
        DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
            fontconfig fonts-noto-cjk fonts-noto-color-emoji fonts-liberation
        if command -v fc-cache >/dev/null 2>&1; then
            fc-cache -f || true
        fi
        echo "[+] CJK fonts installed and font cache refreshed."
    else
        echo "[!] Host CJK fonts not detected (fc-list :lang=zh empty). Install fonts-noto-cjk if running Chromium outside Docker."
    fi
else
    echo "[+] CJK fonts verified via fc-list :lang=zh."
fi

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
