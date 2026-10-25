#!/usr/bin/env pwsh
# 一键构建并启动 RustVNC 配对后台（rv-server）Docker Compose。
# 首次运行会生成 deploy/pairing/pairing.env 与随机管理员令牌。
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
$Compose = Join-Path $Root 'deploy/pairing/docker-compose.yml'
$EnvFile = Join-Path $Root 'deploy/pairing/pairing.env'
$Example = Join-Path $Root 'deploy/pairing/pairing.env.example'

function Get-EnvValue([string]$Name, [string]$Default) {
    $line = Get-Content $EnvFile -ErrorAction SilentlyContinue |
        Where-Object { $_ -match "^$Name=" } | Select-Object -First 1
    if ($line) { return (($line -split '=', 2)[1]).Trim() }
    return $Default
}

function New-AdminToken {
    $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
    $bytes = New-Object byte[] 32
    $rng.GetBytes($bytes)
    return 'admin_' + (($bytes | ForEach-Object { $_.ToString('x2') }) -join '')
}

if (-not (Test-Path $Compose) -or -not (Test-Path $Example)) {
    throw "找不到 $Compose 或 $Example，请在仓库根目录运行本脚本。"
}

if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
    throw '未找到 docker。请安装 Docker Desktop 或 Docker Engine，并确保 docker compose 可用。'
}
docker compose version *> $null
if ($LASTEXITCODE -ne 0) {
    throw '需要 Docker Compose v2（docker compose version 执行失败）。'
}
docker info *> $null
if ($LASTEXITCODE -ne 0) {
    throw 'Docker 守护进程未运行，请先启动 Docker Desktop/Engine。'
}

if (-not (Test-Path $EnvFile)) {
    Copy-Item $Example $EnvFile
    Write-Host "已创建 $EnvFile"
}

$content = [IO.File]::ReadAllText($EnvFile)
$content = $content -replace "`r`n", "`n"
$currentToken = ''
if ($content -match '(?m)^RV_ADMIN_TOKEN=(.*)$') {
    $currentToken = $Matches[1].Trim()
}
if ([string]::IsNullOrWhiteSpace($currentToken) -or $currentToken -eq 'change-me') {
    $token = New-AdminToken
    $content = $content -replace '(?m)^RV_ADMIN_TOKEN=.*$', "RV_ADMIN_TOKEN=$token"
    [IO.File]::WriteAllText(
        $EnvFile,
        $content,
        (New-Object System.Text.UTF8Encoding -ArgumentList $false)
    )
    Write-Host '已生成随机管理员令牌并写入 pairing.env'
} else {
    $token = $currentToken
}

$port = [int](Get-EnvValue 'PAIRING_PORT' '8080')
$bind = Get-EnvValue 'PAIRING_BIND' '127.0.0.1'
$urlHost = if ($bind -eq '0.0.0.0' -or $bind -eq '::') { '127.0.0.1' } else { $bind }
$url = "http://${urlHost}:${port}"

try {
    $listener = Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($listener) {
        $proc = Get-Process -Id $listener.OwningProcess -ErrorAction SilentlyContinue
        Write-Warning "端口 $port 已被 PID $($listener.OwningProcess) ($($proc.ProcessName)) 占用。若是旧的独立 rv-server，请先停止；或修改 $EnvFile 中的 PAIRING_PORT。"
    }
} catch {
    # Get-NetTCPConnection 在极老系统上可能不存在，忽略。
}

Push-Location $Root
try {
    Write-Host '构建并启动配对后台...'
    docker compose --env-file $EnvFile -f $Compose up -d --build
    if ($LASTEXITCODE -ne 0) {
        throw "docker compose 启动失败（exit $LASTEXITCODE）"
    }

    Write-Host '等待 /healthz...'
    $healthy = $false
    for ($i = 0; $i -lt 40; $i++) {
        try {
            Invoke-RestMethod -Uri "$url/healthz" -TimeoutSec 3 | Out-Null
            $healthy = $true
            break
        } catch {
            Start-Sleep -Seconds 2
        }
    }

    if (-not $healthy) {
        Write-Warning "健康检查未通过。查看日志：docker compose --env-file `"$EnvFile`" -f `"$Compose`" logs -f"
    }

    Write-Host ''
    Write-Host "配对后台 : $url"
    Write-Host "管理台   : $url/admin/"
    Write-Host "管理员令牌: $token"
    Write-Host "环境文件 : $EnvFile"
    Write-Host "停止服务 : docker compose --env-file `"$EnvFile`" -f `"$Compose`" down"
    Write-Host ''
    Write-Host "客户端设置中的配对服务器地址填 $url"
} finally {
    Pop-Location
}
