# MC-Tunnel

Minecraft 流量伪装代理隧道。客户端经 MC 加速通道（javaw）连接 VPS，提供 SOCKS5 / HTTP 代理与 GeoIP 分流。
声明:本项目仅供学习交流参考 严禁用于非法用途 可用性取决于您当地法律法规 作者不承担任何后果 请谨慎使用。

## 功能

- MC 握手伪装，隧道多路复用
- 强制经 `javaw` 中继（禁止直连 VPS）
- 三档分流：全部代理 / 仅国外 IP / 全部直连
- 内置 GeoIP 数据库（启动不联网下载）
- Turbo 高速模式、自动重连、隧道健康检测
- Windows .NET 9 图形客户端

## 服务端安装（Linux）

在 VPS 上执行：

```bash
git clone https://github.com/mirror2008/MC-Tunnel.git
cd MC-Tunnel
sudo bash deploy/install.sh
```

按提示输入监听端口（默认 `25565`），端口会保存到 `/opt/mc-tunnel/server.conf`，下次安装可直接回车沿用。

服务管理：

```bash
systemctl status mc-tunnel
systemctl restart mc-tunnel
journalctl -u mc-tunnel -f
```

## Windows 客户端（下载即用）

从 [Releases](https://github.com/mirror2008/MC-Tunnel/releases) 下载 **`MC-Tunnel-Windows-x64.zip`**，解压后运行 **`mc-tunnel-gui.exe`**。

首次使用需安装 [.NET 9 Desktop Runtime](https://dotnet.microsoft.com/download/dotnet/9.0)（约 50MB，一次安装）。

包内包含：`mc-tunnel.exe`、`mc-tunnel-gui.exe`、`data/Country.mmdb`、`bridge/`。

### 自行编译

```powershell
powershell -ExecutionPolicy Bypass -File build-release.ps1
```

输出目录 `dist/`，可打包为 zip 分发。

需要免安装版（内置 .NET，体积大、编译慢约 3 分钟）：

```powershell
powershell -ExecutionPolicy Bypass -File build-release.ps1 -SelfContained
```

### 使用

1. 打开游戏加速器，加速 **Minecraft**
2. 运行 `mc-tunnel-gui.exe`
3. 填写 MC 服务器地址（例：`你的VPS IP:25565`），连接后自动保存配置
4. 速度建议选 **极速 Turbo**，选择分流模式并连接

配置保存在 `%AppData%\MC-Tunnel\mc-tunnel-config.json`。

### 命令行

```powershell
.\mc-tunnel.exe client --remote "1.2.3.4:25565" --local 127.0.0.1:1080 --proxy-mode gfw --speed turbo --set-proxy true
.\mc-tunnel.exe probe --remote "1.2.3.4:25565"
.\mc-tunnel.exe stop
```

## 配置说明

| 字段 | 说明 |
|------|------|
| `remote` | VPS MC 隧道地址 `IP:端口` |
| `local_proxy` | 本地 SOCKS5 地址 |
| `proxy_mode` | `all` / `gfw_only` / `direct` |
| `speed` | `turbo` / `fast` / `balanced` / `stealth` |
| `whitelist` | 国内域名也走代理的列表 |

参考示例：`config/mc-tunnel-config.example.json`

## 目录结构

```
src/              Rust 核心
MC-Tunnel.Gui/    Windows GUI
bridge/           javaw 本地中继
data/             内置 GeoIP 数据库
deploy/install.sh 服务端一键安装
```

## 许可

本项目采用 [GNU General Public License v3.0](LICENSE)（GPL-3.0）。
