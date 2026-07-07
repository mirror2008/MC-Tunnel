# 编译 .NET 9 MC-Tunnel GUI 并发布到 release 目录
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$gui = Join-Path $root "MC-Tunnel.Gui"
$release = Join-Path $root "release"

Get-Process mc-tunnel-gui -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 800

dotnet publish $gui -c Release -r win-x64 --self-contained false -o $release /p:PublishSingleFile=false

# 确保后端与 GUI 同目录
$tunnelSrc = Join-Path $release "mc-tunnel.exe"
if (-not (Test-Path $tunnelSrc)) {
    $built = Join-Path $root "target\release\mc-tunnel.exe"
    if (Test-Path $built) { Copy-Item -Force $built $tunnelSrc }
}

# 内置 GeoIP 数据库
$dataSrc = Join-Path $root "data\Country.mmdb"
$dataDstDir = Join-Path $release "data"
if (Test-Path $dataSrc) {
    New-Item -ItemType Directory -Force -Path $dataDstDir | Out-Null
    Copy-Item -Force $dataSrc (Join-Path $dataDstDir "Country.mmdb")
}

Write-Host "已发布到 $release\mc-tunnel-gui.exe"
