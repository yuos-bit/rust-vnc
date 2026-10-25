#!/usr/bin/env bash
# 一键构建并启动 RustVNC 配对后台（rv-server）Docker Compose。
# 首次运行会生成 deploy/pairing/pairing.env 与随机管理员令牌。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COMPOSE="$ROOT/deploy/pairing/docker-compose.yml"
ENV_FILE="$ROOT/deploy/pairing/pairing.env"
EXAMPLE="$ROOT/deploy/pairing/pairing.env.example"

get_env() {
  local name="$1" default="$2" line
  line="$(grep -E "^${name}=" "$ENV_FILE" 2>/dev/null | head -n1 || true)"
  if [ -n "$line" ]; then
    printf '%s' "${line#*=}" | tr -d '\r'
  else
    printf '%s' "$default"
  fi
}

if [ ! -f "$COMPOSE" ] || [ ! -f "$EXAMPLE" ]; then
  echo "找不到 $COMPOSE 或 $EXAMPLE，请在仓库根目录运行本脚本。" >&2
  exit 1
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "未找到 docker。请安装 Docker Engine/Desktop，并确保 docker compose 可用。" >&2
  exit 1
fi
if ! docker compose version >/dev/null 2>&1; then
  echo "需要 Docker Compose v2（docker compose version 执行失败）。" >&2
  exit 1
fi
if ! docker info >/dev/null 2>&1; then
  echo "Docker 守护进程未运行，请先启动 Docker。" >&2
  exit 1
fi

if [ ! -f "$ENV_FILE" ]; then
  cp "$EXAMPLE" "$ENV_FILE"
  echo "已创建 $ENV_FILE"
fi

current_token="$(get_env RV_ADMIN_TOKEN '')"
if [ -z "$current_token" ] || [ "$current_token" = "change-me" ]; then
  if command -v openssl >/dev/null 2>&1; then
    token="admin_$(openssl rand -hex 32)"
  else
    token="admin_$(head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n')"
  fi
  tmp="$(mktemp)"
  sed "s/^RV_ADMIN_TOKEN=.*$/RV_ADMIN_TOKEN=${token}/" "$ENV_FILE" > "$tmp"
  mv "$tmp" "$ENV_FILE"
  chmod 600 "$ENV_FILE"
  echo "已生成随机管理员令牌并写入 pairing.env"
else
  token="$current_token"
fi

port="$(get_env PAIRING_PORT '8080')"
bind="$(get_env PAIRING_BIND '127.0.0.1')"
url_host="$bind"
if [ "$bind" = "0.0.0.0" ] || [ "$bind" = "::" ]; then
  url_host="127.0.0.1"
fi
url="http://${url_host}:${port}"

cd "$ROOT"
echo "构建并启动配对后台..."
docker compose --env-file "$ENV_FILE" -f "$COMPOSE" up -d --build

if command -v curl >/dev/null 2>&1; then
  echo "等待 /healthz ..."
  healthy=0
  i=0
  while [ "$i" -lt 40 ]; do
    if curl -fsS "$url/healthz" >/dev/null 2>&1; then
      healthy=1
      break
    fi
    sleep 2
    i=$((i + 1))
  done
  if [ "$healthy" -ne 1 ]; then
    echo "警告：健康检查未通过。查看日志：" >&2
    echo "  docker compose --env-file \"$ENV_FILE\" -f \"$COMPOSE\" logs -f" >&2
  fi
else
  echo "未找到 curl，跳过健康检查；可手动访问 $url/healthz"
fi

echo
echo "配对后台 : $url"
echo "管理台   : $url/admin/"
echo "管理员令牌: $token"
echo "环境文件 : $ENV_FILE"
echo "停止服务 : docker compose --env-file \"$ENV_FILE\" -f \"$COMPOSE\" down"
echo
echo "客户端设置中的配对服务器地址填 $url"
