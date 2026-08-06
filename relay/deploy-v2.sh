#!/usr/bin/env bash
set -euo pipefail

APP_DIR="/home/ubuntu/bob-relay"
BACKUP_NAME="backup-$(date -u +%Y%m%d-%H%M%S)"
BACKUP_DIR="$APP_DIR/$BACKUP_NAME"

cd "$APP_DIR"
mkdir -p "$BACKUP_DIR/src"
cp src/server.js "$BACKUP_DIR/src/server.js"
cp package.json package-lock.json "$BACKUP_DIR/"

install -m 664 /tmp/bob-relay-server.v2.js src/server.js
install -m 664 /tmp/bob-relay-package.v2.json package.json
install -m 664 /tmp/bob-relay-package-lock.v2.json package-lock.json
npm ci --omit=dev

old_pid="$(pgrep -f '^node src/server.js$' | head -1 || true)"
if [[ -n "$old_pid" ]]; then
  kill "$old_pid"
fi

nohup node src/server.js > server.log 2>&1 &
new_pid=$!
healthy=0
for _ in {1..10}; do
  sleep 1
  if curl -fsS http://127.0.0.1:3090/status > /tmp/bob-relay-status.v2.json; then
    healthy=1
    break
  fi
done

if [[ "$healthy" -ne 1 ]]; then
  kill "$new_pid" 2>/dev/null || true
  cp "$BACKUP_DIR/src/server.js" src/server.js
  cp "$BACKUP_DIR/package.json" "$BACKUP_DIR/package-lock.json" .
  npm ci --omit=dev
  nohup node src/server.js > server.log 2>&1 &
  echo "ROLLED_BACK backup=$BACKUP_NAME"
  exit 1
fi

echo "DEPLOYED backup=$BACKUP_NAME pid=$new_pid"
cat /tmp/bob-relay-status.v2.json
tail -n 8 server.log
