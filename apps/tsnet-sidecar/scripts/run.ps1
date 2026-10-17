#Requires -Version 5.1
# 启动 RustVNC tsnet sidecar（Windows）。
# 只设置非敏感的默认值；密钥必须由调用者通过环境变量提供。

$ErrorActionPreference = "Stop"

if (-not $env:TS_AUTHKEY) {
    Write-Host "错误：请先设置 TS_AUTHKEY（Headscale 预授权密钥）。" -ForegroundColor Red
    exit 1
}

$Root = Split-Path -Parent $PSScriptRoot

if (-not $env:TS_HOSTNAME) { $env:TS_HOSTNAME = "rvnc-sidecar" }
if (-not $env:TS_STATE_DIR) { $env:TS_STATE_DIR = Join-Path $Root "tsnet-state" }
if (-not $env:RVNC_AGENT_ADDR) { $env:RVNC_AGENT_ADDR = "127.0.0.1:21416" }
if (-not $env:RVNC_LISTEN_ADDR) { $env:RVNC_LISTEN_ADDR = ":21416" }
if (-not $env:RVNC_SOCKS_ADDR) { $env:RVNC_SOCKS_ADDR = "127.0.0.1:1055" }
if (-not $env:RVNC_STATUS_ADDR) { $env:RVNC_STATUS_ADDR = "127.0.0.1:1056" }

Write-Host "RustVNC tsnet sidecar" -ForegroundColor Cyan
Write-Host "  hostname : $env:TS_HOSTNAME"
Write-Host "  state    : $env:TS_STATE_DIR"
Write-Host "  agent    : $env:RVNC_AGENT_ADDR"
Write-Host "  socks5   : $env:RVNC_SOCKS_ADDR"
Write-Host "  status   : $env:RVNC_STATUS_ADDR"

$exe = Join-Path $Root "rvnc-tsnet.exe"
if (Test-Path -LiteralPath $exe) {
    & $exe
    exit $LASTEXITCODE
}

$go = Get-Command go -ErrorAction SilentlyContinue
if ($go) {
    Push-Location $Root
    try {
        & go run .
    }
    finally {
        Pop-Location
    }
    exit $LASTEXITCODE
}

Write-Host "错误：未找到 rvnc-tsnet.exe，且未安装 Go。请先在 apps/tsnet-sidecar 执行: go mod tidy; go build -o rvnc-tsnet.exe ." -ForegroundColor Red
exit 1
