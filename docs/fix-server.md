# VPS 服务端修复（连接失败时执行）

客户端 `probe` 显示「端口可达但无 MC 响应」= **服务器上 mc-tunnel 没在跑**，不是 25565 被墙。

## 1. SSH 登录 VPS

```bash
ssh root@你的服务器IP
```

## 2. 换端口并重启（推荐 25568）

```bash
cd /root/MC-Tunnel || cd MC-Tunnel
git pull
sudo bash deploy/change-port.sh
# 输入: 25568
```

## 3. 确认服务正常

```bash
systemctl status mc-tunnel
ss -tlnp | grep 25568
```

应看到 `mc-tunnel` 在 `0.0.0.0:25568` 监听。

## 4. 本地验证（Windows）

```powershell
cd dist2
.\mc-tunnel.exe probe --remote 你的IP:25568
```

必须显示 **「探测成功」** 后再用 GUI，地址填 `你的IP:25568`。

## 首次安装

```bash
git clone https://github.com/mirror2008/MC-Tunnel.git
cd MC-Tunnel
sudo bash deploy/install.sh
# 端口填 25568
```
