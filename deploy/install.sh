#!/bin/bash
# MC-Tunnel 服务端一键安装（在 Linux VPS 上运行）
# 用法:
#   git clone https://github.com/mirror2008/MC-Tunnel.git && cd MC-Tunnel && sudo bash deploy/install.sh
set -euo pipefail

APP_DIR="/opt/mc-tunnel"
BIN="$APP_DIR/mc-tunnel"
CONFIG="$APP_DIR/server.conf"
SERVICE="/etc/systemd/system/mc-tunnel.service"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

if [[ "$(id -u)" -ne 0 ]]; then
  echo "请使用 root 运行: sudo bash deploy/install.sh"
  exit 1
fi

# 读取已保存的端口
SAVED_PORT=""
if [[ -f "$CONFIG" ]]; then
  # shellcheck disable=SC1090
  source "$CONFIG"
  SAVED_PORT="${LISTEN_PORT:-}"
fi

DEFAULT_PORT="${SAVED_PORT:-25565}"
read -r -p "MC-Tunnel 监听端口 [${DEFAULT_PORT}]: " INPUT_PORT
LISTEN_PORT="${INPUT_PORT:-$DEFAULT_PORT}"

if ! [[ "$LISTEN_PORT" =~ ^[0-9]+$ ]] || [[ "$LISTEN_PORT" -lt 1 || "$LISTEN_PORT" -gt 65535 ]]; then
  echo "无效端口: $LISTEN_PORT"
  exit 1
fi

echo "LISTEN_PORT=$LISTEN_PORT" > "$CONFIG"
echo "SPEED=fast" >> "$CONFIG"
chmod 600 "$CONFIG"

echo "==> 安装编译依赖..."
if command -v apt-get >/dev/null 2>&1; then
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -qq
  apt-get install -y -qq build-essential pkg-config curl ca-certificates
elif command -v dnf >/dev/null 2>&1; then
  dnf install -y gcc gcc-c++ make curl ca-certificates
elif command -v yum >/dev/null 2>&1; then
  yum install -y gcc gcc-c++ make curl ca-certificates
else
  echo "警告: 未识别的包管理器，请手动安装 gcc / make / curl"
fi

echo "==> 检查 Rust 工具链..."
if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
# shellcheck disable=SC1091
source "$HOME/.cargo/env" 2>/dev/null || export PATH="$HOME/.cargo/bin:$PATH"

echo "==> 编译 MC-Tunnel 服务端..."
cd "$PROJECT_ROOT"
cargo build --release --no-default-features --bin mc-tunnel

mkdir -p "$APP_DIR"
cp "$PROJECT_ROOT/target/release/mc-tunnel" "$BIN"
chmod +x "$BIN"

cat > "$SERVICE" << EOF
[Unit]
Description=MC-Tunnel Minecraft Camouflage Proxy Server
After=network.target

[Service]
Type=simple
User=root
WorkingDirectory=$APP_DIR
ExecStart=$BIN server --listen 0.0.0.0:${LISTEN_PORT} --speed fast
Restart=always
RestartSec=3
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable mc-tunnel
systemctl restart mc-tunnel

if command -v ufw >/dev/null 2>&1; then
  ufw allow "${LISTEN_PORT}/tcp" || true
elif command -v firewall-cmd >/dev/null 2>&1; then
  firewall-cmd --permanent --add-port="${LISTEN_PORT}/tcp" || true
  firewall-cmd --reload || true
fi

sleep 1
echo ""
echo "=========================================="
echo " MC-Tunnel 服务端已安装"
echo " 监听端口: $LISTEN_PORT"
echo " 配置已保存: $CONFIG"
echo " 客户端填写: <你的服务器IP>:$LISTEN_PORT"
echo "=========================================="
systemctl status mc-tunnel --no-pager || true
ss -tlnp | grep ":$LISTEN_PORT" || netstat -tlnp 2>/dev/null | grep ":$LISTEN_PORT" || true
