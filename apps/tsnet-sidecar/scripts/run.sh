#!/usr/bin/env sh
# 启动 RustVNC tsnet sidecar（Linux/macOS）。
# 只设置非敏感的默认值；密钥必须由调用者通过环境变量提供。
set -eu

if [ -z "${TS_AUTHKEY:-}" ]; then
    echo "错误：请先设置 TS_AUTHKEY（Headscale 预授权密钥）。" >&2
    exit 1
fi

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

: "${TS_HOSTNAME:=rvnc-sidecar}"
: "${TS_STATE_DIR:=$ROOT/tsnet-state}"
: "${RVNC_AGENT_ADDR:=127.0.0.1:21416}"
: "${RVNC_LISTEN_ADDR:=:21416}"
: "${RVNC_SOCKS_ADDR:=127.0.0.1:1055}"
: "${RVNC_STATUS_ADDR:=127.0.0.1:1056}"

export TS_HOSTNAME TS_STATE_DIR RVNC_AGENT_ADDR RVNC_LISTEN_ADDR RVNC_SOCKS_ADDR RVNC_STATUS_ADDR

echo "RustVNC tsnet sidecar"
echo "  hostname : $TS_HOSTNAME"
echo "  state    : $TS_STATE_DIR"
echo "  agent    : $RVNC_AGENT_ADDR"
echo "  socks5   : $RVNC_SOCKS_ADDR"
echo "  status   : $RVNC_STATUS_ADDR"

if [ -x "$ROOT/rvnc-tsnet" ]; then
    exec "$ROOT/rvnc-tsnet"
fi

if command -v go >/dev/null 2>&1; then
    cd "$ROOT"
    exec go run .
fi

echo "错误：未找到 rvnc-tsnet，且未安装 Go。请先在 apps/tsnet-sidecar 执行: go mod tidy; go build -o rvnc-tsnet ." >&2
exit 1
