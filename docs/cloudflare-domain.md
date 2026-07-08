# 用 Cloudflare 域名 + 海外服务器跑 MC-Tunnel

## 先说清楚：Workers **不能**当 MC-Tunnel 服务端

MC-Tunnel 服务端需要：

| 能力 | MC-Tunnel 服务端 | Cloudflare Workers |
|------|------------------|-------------------|
| 监听 | **原始 TCP**（如 `25568`） | 只有 HTTP / WebSocket |
| 协议 | **Minecraft 握手伪装** | 不支持 |
| MC 加速通道 | `javaw` 打到 **IP:端口** | 没有固定 TCP 端口 |

所以：**现有 MC-Tunnel 客户端 + 游戏加速器，必须用一台能跑 `mc-tunnel server` 的 Linux 机器**（VPS）。

你的域名在 Cloudflare 上最有用的方式是：**DNS 指向新 VPS**，不是 Workers。

---

## 推荐方案：域名 + 新 VPS（真正能用）

### 1. 买一台海外 VPS

任意便宜 Linux 即可（香港、新加坡、美国等），例如：

- Oracle Cloud 免费档
- Vultr / RackNerd / Bandwagon 等

### 2. Cloudflare DNS 设置

假设域名 `example.com`，要用的子域 `mc.example.com`：

| 类型 | 名称 | 内容 | 代理状态 |
|------|------|------|----------|
| **A** | `mc` | `新VPS的IP` | **仅 DNS（灰云）** |

⚠️ 必须 **关闭橙色代理（灰云）**。MC-Tunnel 是自定义 TCP 端口，不能走 CF 的 HTTP 代理。

### 3. VPS 上安装 MC-Tunnel

```bash
git clone https://github.com/mirror2008/MC-Tunnel.git
cd MC-Tunnel
sudo bash deploy/install.sh
# 端口建议: 25568
```

### 4. 防火墙放行

```bash
ufw allow 25568/tcp
```

### 5. 客户端填写

GUI 里填：

```
mc.example.com:25568
```

本地验证：

```powershell
.\mc-tunnel.exe probe --remote mc.example.com:25568
```

必须 **探测成功** 再连接。

---

## Workers 能做什么（附赠，不是 MC-Tunnel）

仓库里有 `cloudflare/worker-http-proxy/`：一个简单的 **HTTP 代理 Worker**。

- ✅ 适合：浏览器/工具走 HTTPS 代理（不经 MC 加速通道）
- ❌ 不能：替代 `mc-tunnel server`，不能接 javaw MC 加速 TCP

### 部署 Worker

```bash
cd cloudflare/worker-http-proxy
npm i -g wrangler
wrangler login
# 编辑 wrangler.toml 里的 routes / 域名
wrangler deploy
```

### 使用（明文转发模式）

```
GET https://mc-proxy.example.com/?url=https://www.google.com
```

---

## 总结

| 目标 | 方案 |
|------|------|
| MC-Tunnel + 游戏加速器 | **VPS + 域名 A 记录（灰云）** |
| 只要简单翻墙、不用 MC 加速 | 可用 **Workers HTTP 代理**（需另配客户端） |

旧服务器 `156.239.41.19` 已挂，请换新 VPS 后按上面步骤重装。
