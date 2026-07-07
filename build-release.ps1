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

# 清理旧版中文文件名（zip 内易乱码）
Get-ChildItem $release -File | Where-Object { $_.Extension -in '.bat','.txt' -and $_.Name -notmatch '^[A-Za-z0-9._-]+$' } | Remove-Item -Force -ErrorAction SilentlyContinue

$readme = @"
MC-Tunnel Windows Client
========================

1. Install .NET 9 Desktop Runtime (first time only):
   https://dotnet.microsoft.com/download/dotnet/9.0
2. Start Leigod accelerator for Minecraft
3. Run mc-tunnel-gui.exe
4. Enter VPS address (IP:port); settings save to mc-tunnel-config.json

License: GPL-3.0
"@
[System.IO.File]::WriteAllText((Join-Path $release "README.txt"), $readme, [System.Text.UTF8Encoding]::new($true))

$zip = Join-Path $root "MC-Tunnel-Windows-x64.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Write-Host "==> 打包 zip..."
Compress-Archive -Path "$release\*" -DestinationPath $zip -Force

Write-Host ""
Write-Host "完成! dist\  压缩包: $zip"
Write-Host "启动: $release\mc-tunnel-gui.exe"
