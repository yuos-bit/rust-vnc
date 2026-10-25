# 配对后台一键启动（Docker Compose）

本目录只启动 **配对后台 `rv-server`**，不包含 Headscale、DERP、Caddy。适合本机联调或单独部署配对服务。

> 注意：本目录的 `pairing.env` 是**服务端配对后台**配置；仓库根目录的 `env-back` / `.env` 是**客户端构建时内置**的配置模板，两者不要混用。完整生产栈（Headscale + DERP + Caddy TLS + 配对后台）见 [`deploy/docker-compose.yml`](../docker-compose.yml) 与 [`docs/DEPLOYMENT.md`](../../docs/DEPLOYMENT.md)。

## 1. 前置条件

- Docker Engine 24+ 或 Docker Desktop
- Docker Compose v2（`docker compose version` 可用）
- 端口 `8080` 未被占用（或修改 `PAIRING_PORT`）

## 2. 一键启动（推荐）

### Windows

```powershell
.\scripts\start-pairing-backend.ps1
```

如果 PowerShell 阻止脚本：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\start-pairing-backend.ps1
```

### Linux / macOS

```bash
chmod +x scripts/start-pairing-backend.sh
./scripts/start-pairing-backend.sh
```

脚本会：

1. 检查 Docker 与 Compose；
2. 首次运行时复制 `pairing.env.example` → `pairing.env`，生成随机 `RV_ADMIN_TOKEN`；
3. 构建并后台启动 `rv-server`；
4. 等待 `/healthz` 通过；
5. 打印管理台地址与管理员令牌。

## 3. 手动启动

```bash
cp deploy/pairing/pairing.env.example deploy/pairing/pairing.env
chmod 600 deploy/pairing/pairing.env

# 编辑 pairing.env，至少替换 RV_ADMIN_TOKEN
# 可选：PAIRING_BIND / PAIRING_PORT / HEADSCALE_*

docker compose --env-file deploy/pairing/pairing.env \
  -f deploy/pairing/docker-compose.yml up -d --build
```

验证：

```bash
curl -s http://127.0.0.1:8080/healthz
# {"ok":true,"api":"v1","version":"0.1.0"}
```

管理台：<http://127.0.0.1:8080/admin/>（输入 `RV_ADMIN_TOKEN`）。

## 4. 配置说明

| 变量 | 默认 | 说明 |
| --- | --- | --- |
| `RV_ADMIN_TOKEN` | 无（必填） | 管理台/管理 API 的 Bearer 令牌，建议 32+ 字符 |
| `PAIRING_BIND` | `127.0.0.1` | 宿主机监听地址；`0.0.0.0` 表示对外 |
| `PAIRING_PORT` | `8080` | 宿主机端口 |
| `PUBLIC_URL` | 空 | 服务对外地址，写入设置 |
| `HEADSCALE_URL` | 空 | Headscale API 地址（可选） |
| `HEADSCALE_API_KEY` | 空 | Headscale API Key（可选，仅服务端使用） |
| `HEADSCALE_USER` | `1` | Headscale 用户 ID |
| `HEADSCALE_CONTROL_URL` | 空 | 下发给客户端的控制面地址（留空回退 `HEADSCALE_URL`） |
| `DERP_URL` | 空 | 下发给客户端的 DERP 地址 |
| `AGENT_PORT` | `21416` | 被控端 tailnet 端口 |
| `TOKEN_TTL_DAYS` | `30` | 免密令牌有效期 |
| `RV_TRUST_PROXY` | `0` | 仅在可信反向代理后设为 `1` |

客户端（PC/Android）的设置中，配对服务器地址填：

- 本机：`http://127.0.0.1:8080`
- 远程：`https://<你的 rv-server 域名>`（必须由 Caddy/Nginx 提供 TLS）

## 5. 数据、日志与更新

数据保存在命名卷 `rustvnc-pairing_rvnc-pairing-data`（SQLite），删除容器不会丢失：

```bash
# 查看日志
docker compose -f deploy/pairing/docker-compose.yml logs -f rv-server

# 停止（保留数据）
docker compose -f deploy/pairing/docker-compose.yml down

# 更新代码后重建
docker compose --env-file deploy/pairing/pairing.env \
  -f deploy/pairing/docker-compose.yml up -d --build

# 清空数据（谨慎）
docker compose -f deploy/pairing/docker-compose.yml down -v
```

备份 SQLite：

```bash
docker run --rm -v rustvnc-pairing_rvnc-pairing-data:/data \
  -v "$PWD":/backup debian:bookworm-slim \
  sh -c 'cp /data/rvnc.db /backup/rvnc-$(date +%Y%m%d-%H%M%S).db'
```

## 6. 端口冲突

如果本机已有独立运行的 `rv-server` 占用 `8080`，Compose 会启动失败。二选一：

- 停止旧的 `rv-server`；
- 在 `pairing.env` 中修改 `PAIRING_PORT`（例如 `18080`），并在客户端设置中同步修改服务器地址。

> 切换数据库后，旧客户端的设备身份/令牌属于旧库。请在 Windows 被控端点击 **“重新注册”**，或让控制端重新配对。

## 7. 安全

- 默认只绑定 `127.0.0.1`；不要把明文 `8080` 直接暴露到公网。
- 对外部署请设置 `PAIRING_BIND=0.0.0.0`，并在前面使用 Caddy/Nginx 终止 TLS。
- `pairing.env` 含管理员令牌与 Headscale API Key，权限设为 `600`，不要提交。
- 管理员令牌泄露等同于管理台完全控制权；请定期轮换。
