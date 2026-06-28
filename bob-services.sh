#!/bin/bash
# ═══════════════════════════════════════════════════════════════
# bob-services.sh — Bob VPS 服务管理脚本
#
# 用法:
#   ./bob-services.sh setup   — 首次安装（coturn + 环境检查）
#   ./bob-services.sh reset   — 干掉所有旧进程，重新拉起全部服务
#   ./bob-services.sh status  — 查看所有服务状态
#   ./bob-services.sh stop    — 停止所有服务
# ═══════════════════════════════════════════════════════════════

set -e

RELAY_DIR="/home/ubuntu/bob"
RELAY_SCRIPT="$RELAY_DIR/bob-relay.js"
RELAY_LOG="/home/ubuntu/bob/relay.log"
COTURN_SECRET="T3zytkYeHfLiHPXu1lJratBZZFIsYH+DhnUjscxlBHI="

# ─── 确保 nvm/node 在 PATH 中（sudo 环境下需要手动 source）───
export NVM_DIR="/home/ubuntu/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && source "$NVM_DIR/nvm.sh" 2>/dev/null
NODE_BIN=$(command -v node 2>/dev/null || echo "/home/ubuntu/.nvm/versions/node/v22.22.0/bin/node")

# ─── 颜色 ───
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

ok()   { echo -e "${GREEN}✅ $1${NC}"; }
fail() { echo -e "${RED}❌ $1${NC}"; }
info() { echo -e "${YELLOW}⏳ $1${NC}"; }

# ─── 停止所有 Bob 相关进程 ───
kill_all() {
    info "正在停止所有 Bob 相关进程..."
    
    # 1. 干掉所有 bob-relay node 进程
    pkill -f "node.*bob-relay" 2>/dev/null && echo "   killed bob-relay" || echo "   bob-relay 未在运行"
    
    # 2. 干掉 coturn（如果是手动启动的）
    pkill -f "turnserver" 2>/dev/null && echo "   killed turnserver" || echo "   turnserver 未在运行"
    
    # 等一秒确保进程退出
    sleep 1
}

# ─── 启动 bob-relay ───
start_relay() {
    if [ ! -f "$RELAY_SCRIPT" ]; then
        fail "找不到 $RELAY_SCRIPT"
        return 1
    fi
    
    info "启动 bob-relay..."
    cd "$RELAY_DIR"
    nohup "$NODE_BIN" "$RELAY_SCRIPT" >> "$RELAY_LOG" 2>&1 &
    sleep 1
    
    if pgrep -f "node.*bob-relay" > /dev/null; then
        ok "bob-relay 已启动 (PID: $(pgrep -f 'node.*bob-relay'))"
    else
        fail "bob-relay 启动失败，查看日志: $RELAY_LOG"
        tail -5 "$RELAY_LOG" 2>/dev/null
    fi
}

# ─── 启动 coturn ───
start_coturn() {
    if ! command -v turnserver &> /dev/null; then
        fail "coturn 未安装，请先运行: ./bob-services.sh setup"
        return 1
    fi
    
    # 用 systemctl 管理（如果之前 setup 过）
    if systemctl is-active coturn &> /dev/null; then
        ok "coturn 已在运行 (systemd)"
        return 0
    fi
    
    info "启动 coturn..."
    sudo systemctl start coturn 2>/dev/null || {
        # systemd 不可用时手动启动
        nohup turnserver -c /etc/turnserver.conf >> /var/log/coturn.log 2>&1 &
    }
    sleep 1
    
    if pgrep -f "turnserver" > /dev/null; then
        ok "coturn 已启动 (PID: $(pgrep -f 'turnserver'))"
    else
        fail "coturn 启动失败"
    fi
}

# ─── 查看状态 ───
show_status() {
    echo ""
    echo "═══ Bob VPS 服务状态 ═══"
    echo ""
    
    # bob-relay
    if pgrep -f "node.*bob-relay" > /dev/null; then
        ok "bob-relay     运行中 (PID: $(pgrep -f 'node.*bob-relay' | head -1), 端口: 3900)"
    else
        fail "bob-relay     未运行"
    fi
    
    # coturn
    if pgrep -f "turnserver" > /dev/null; then
        ok "coturn        运行中 (PID: $(pgrep -f 'turnserver' | head -1), 端口: 3478)"
    elif command -v turnserver &> /dev/null; then
        fail "coturn        已安装但未运行"
    else
        fail "coturn        未安装"
    fi
    
    # Caddy
    if pgrep -f "caddy" > /dev/null; then
        ok "caddy         运行中"
    else
        fail "caddy         未运行"
    fi
    
    # 前端文件
    if [ -f "/opt/test/drop/index.html" ]; then
        ok "web-drop 前端  已部署 ($(stat -c%s /opt/test/drop/index.html) bytes)"
    else
        fail "web-drop 前端  未部署"
    fi
    
    echo ""
}

# ─── 首次安装 ───
do_setup() {
    echo ""
    echo "═══ Bob VPS 首次安装 ═══"
    echo ""
    
    # 1. 安装 coturn
    if command -v turnserver &> /dev/null; then
        ok "coturn 已安装，跳过"
    else
        info "安装 coturn..."
        sudo apt-get update -qq
        sudo apt-get install -y -qq coturn
        ok "coturn 安装完成"
    fi
    
    # 2. 写入 coturn 配置
    info "写入 coturn 配置..."
    sudo tee /etc/turnserver.conf > /dev/null << EOF
# Bob WebRTC STUN/TURN
listening-port=3478
realm=bob.bobbik.org
use-auth-secret
static-auth-secret=${COTURN_SECRET}
no-multicast-peers
no-cli
EOF
    ok "配置写入 /etc/turnserver.conf"
    
    # 3. 启用 coturn 守护进程
    sudo sed -i 's/#TURNSERVER_ENABLED=1/TURNSERVER_ENABLED=1/' /etc/default/coturn 2>/dev/null || true
    sudo systemctl enable coturn 2>/dev/null || true
    sudo systemctl restart coturn 2>/dev/null || true
    ok "coturn 服务已启用"
    
    # 4. 检查 bob-relay 依赖
    if [ -f "$RELAY_DIR/node_modules/ws/lib/websocket.js" ]; then
        ok "bob-relay 依赖已安装"
    else
        info "安装 bob-relay 依赖..."
        cd "$RELAY_DIR" && npm install --production
        ok "bob-relay 依赖安装完成"
    fi
    
    echo ""
    ok "安装完成！现在运行 ./bob-services.sh reset 来启动所有服务。"
}

# ─── 主入口 ───
case "${1:-status}" in
    setup)
        do_setup
        ;;
    reset)
        kill_all
        start_relay
        start_coturn
        echo ""
        show_status
        ;;
    stop)
        kill_all
        show_status
        ;;
    status)
        show_status
        ;;
    *)
        echo "用法: $0 {setup|reset|status|stop}"
        echo ""
        echo "  setup   首次安装 coturn + 环境检查"
        echo "  reset   干掉所有旧进程，重新拉起全部服务"
        echo "  status  查看服务状态"
        echo "  stop    停止所有服务"
        exit 1
        ;;
esac
