#!/bin/bash
# 快速更换 MC-Tunnel 服务端监听端口（无需重新编译）
set -euo pipefail

APP_DIR="/opt/mc-tunnel"
BIN="$APP_DIR/mc-tunnel"
CONFIG="$APP_DIR/server.conf"
SERVICE="/etc/systemd/system/mc-tunnel.service"

if [[ "$(id -u)" -ne 0 ]]; then
  echo "请使用 root 运行: sudo bash deploy/change-port.sh"
  exit 1
fi

CURRENT="25565"
if [[ -f "$CONFIG" ]]; then
  # shellcheck disable=SC1090
  source "$CONFIG"
  CURRENT="${LISTEN_PORT:-25565}"
fi

read -r -p "新监听端口 [建议 25568，当前 ${CURRENT}]: " INPUT_PORT
LISTEN_PORT="${INPUT_PORT:-25568}"

if ! [[ "$LISTEN_PORT" =~ ^[0-9]+$ ]] || [[ "$LISTEN_PORT" -lt 1 || "$LISTEN_PORT" -gt 65535 ]]; then
  echo "无效端口: $LISTEN_PORT"
  exit 1
fi

if [[ ! -x "$BIN" ]]; then
  echo "未找到 $BIN，请先运行 deploy/install.sh"
  exit 1
fi

echo "LISTEN_PORT=$LISTEN_PORT" > "$CONFIG"
echo "SPEED=fast" >> "$CONFIG"
chmod 600 "$CONFIG"

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
systemctl restart mc-tunnel

if command -v ufw >/dev/null 2>&1; then
  ufw allow "${LISTEN_PORT}/tcp" || true
elif command -v firewall-cmd >/dev/null 2>&1; then
  firewall-cmd --permanent --add-port="${LISTEN_PORT}/tcp" || true
  firewall-cmd --reload || true
fi

echo ""
echo "端口已更换为 $LISTEN_PORT"
echo "客户端填写: <服务器IP>:$LISTEN_PORT"
systemctl status mc-tunnel --no-pager || true
ss -tlnp | grep ":$LISTEN_PORT" || true
