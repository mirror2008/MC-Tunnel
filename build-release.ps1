# MC-Tunnel 快速发布（默认依赖系统 .NET 9，编译约 20 秒）
# 需要免安装版: powershell -File build-release.ps1 -SelfContained
param([switch]$SelfContained)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$release = Join-Path $root "dist"
$gui = Join-Path $root "MC-Tunnel.Gui"

Write-Host "==> 停止占用进程..."
Get-Process mc-tunnel-gui, mc-tunnel -ErrorAction SilentlyContinue | ForEach-Object { try { $_.Kill($true) } catch {} }
Start-Sleep -Milliseconds 400

Write-Host "==> 编译 Rust 后端..."
Push-Location $root
cargo build --release --bin mc-tunnel
if ($LASTEXITCODE -ne 0) { Pop-Location; exit $LASTEXITCODE }
Pop-Location

if (Test-Path $release) {
    Get-ChildItem $release -Exclude "data","bridge" | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
} else {
    New-Item -ItemType Directory -Force -Path $release | Out-Null
}

$sc = if ($SelfContained) { "true" } else { "false" }
Write-Host "==> 发布 GUI (self-contained=$sc)..."
dotnet publish $gui -c Release -r win-x64 --self-contained $sc -o $release /p:PublishSingleFile=false
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Copy-Item -Force (Join-Path $root "target\release\mc-tunnel.exe") (Join-Path $release "mc-tunnel.exe")

New-Item -ItemType Directory -Force -Path (Join-Path $release "data") | Out-Null
Copy-Item -Force (Join-Path $root "data\Country.mmdb") (Join-Path $release "data\Country.mmdb") -ErrorAction SilentlyContinue
if (-not (Test-Path (Join-Path $release "bridge\McLeigodBridge.class"))) {
    Copy-Item -Recurse -Force (Join-Path $root "bridge") (Join-Path $release "bridge")
}

$configDst = Join-Path $release "mc-tunnel-config.json"
if (-not (Test-Path $configDst)) {
    Copy-Item -Force (Join-Path $root "config\mc-tunnel-config.example.json") $configDst
}

@'
@echo off
chcp 65001 >nul
where dotnet >nul 2>&1
if errorlevel 1 (
  echo 未检测到 .NET 运行时。请安装 .NET 9 Desktop Runtime:
  echo https://dotnet.microsoft.com/download/dotnet/9.0
  pause
  exit /b 1
)
start "" "%~dp0mc-tunnel-gui.exe"
'@ | Set-Content -Path (Join-Path $release "启动.bat") -Encoding ASCII

Set-Content -Path (Join-Path $release "使用说明.txt") -Encoding UTF8 -Value @"
MC-Tunnel Windows 客户端
========================

1. 安装 .NET 9 Desktop Runtime（仅首次）
   https://dotnet.microsoft.com/download/dotnet/9.0
2. 打开雷神 → 加速 Minecraft
3. 双击 启动.bat 或 mc-tunnel-gui.exe
4. 填写 VPS 地址（IP:端口），配置自动保存

免安装大包: 在源码目录运行 build-release.ps1 -SelfContained（较慢）

许可: GPL-3.0
"@

$zip = Join-Path $root "MC-Tunnel-Windows-x64.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Write-Host "==> 打包 zip..."
Compress-Archive -Path "$release\*" -DestinationPath $zip -Force

Write-Host ""
Write-Host "完成! dist\  压缩包: $zip"
Write-Host "启动: $release\启动.bat"
