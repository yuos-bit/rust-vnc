# RustVNC 服务端部署

本目录包含 RustVNC 的服务端编排：Headscale 控制面、自建 DERP（derper）、配对后台 `rv-server` 与 Caddy TLS。

> 只需要配对后台？使用最小一键 Compose：[deploy/pairing/README.md](pairing/README.md)
> 或运行 `scripts/start-pairing-backend.ps1` / `scripts/start-pairing-backend.sh`。

## 快速开始

```bash
cd deploy
cp .env.example .env
chmod 600 .env

# rv-server 以 UID 10001 运行，需要可写 data/
mkdir -p data backup
sudo chown 10001:10001 data
chmod 700 data

# 1. 修改 .env（域名、管理员令牌等；HEADSCALE_API_KEY 先留空）
# 2. 修改 headscale/config.yaml（server_url、derp.urls、dns.base_domain）
# 3. 准备 derper 证书（见 derp/certs/README.md），或让 derper 独占 80/443 使用 ACME

# 4. 先只启动 Headscale
docker compose up -d headscale

# 5. 创建 Headscale 用户与 API Key，并回填 .env
docker compose exec headscale headscale users create rvnc
docker compose exec headscale headscale users list
#   把用户 ID 填入 .env 的 HEADSCALE_USER

docker compose exec headscale headscale apikeys create --expiration 90d
#   把密钥填入 .env 的 HEADSCALE_API_KEY

# 6. 启动全部服务
docker compose up -d --build
```

验证：

```bash
curl -s https://rvnc.example.com/healthz
# {"ok":true,"api":"v1","version":"0.1.0"}

ADMIN_TOKEN='<管理员令牌>'
curl -s -H "Authorization: Bearer $ADMIN_TOKEN" \
  https://rvnc.example.com/api/admin/stats
```

管理台：`https://rvnc.example.com/admin/`

## 组件与端口

| 组件 | 容器内端口 | 宿主机端口 | 说明 |
| --- | --- | --- | --- |
| Caddy | 80, 443 | 80/tcp, 443/tcp, 443/udp | 自动 HTTPS，反代 rv-server 与 Headscale |
| rv-server | 8080 | 不发布 | 配对 API + Web 管理台 |
| Headscale | 8080, 9090 | 不发布 | 控制面；metrics 仅回环 |
| derper | 443/tcp, 3478/udp | 8443/tcp, 3478/udp | DERP 中继；**禁止反向代理** |

单机部署时 Caddy 与 derper 都需要 443；因此 derper 在宿主机默认发布为 **8443**，`DERP_URL` 也必须带端口。若 derper 使用独立主机/独立公网 IP，把 `DERP_HTTPS_PORT` 改为 `443` 并去掉 URL 中的端口。

## 文件说明

```text
deploy/
├── .env.example              # 环境变量模板（复制为 .env）
├── docker-compose.yml        # 四个服务 + 持久化卷
├── Caddyfile                 # TLS 终止与反向代理
├── server.Dockerfile         # rv-server 多阶段构建
├── data/                     # rv-server SQLite、Caddy 证书（运行时生成）
├── derp/
│   ├── Dockerfile            # 从 tailscale.com/cmd/derper 构建
│   └── certs/                # manual 证书（derp.example.com.crt/.key）
└── headscale/
    ├── config.yaml           # Headscale 配置模板
    └── README.md             # 用户/API Key/审批/运维
```

## 关键约定

- 被控端 tailnet 主机名：`rvnc-<6 位设备码小写>`，例如 `rvnc-a2b3c4`。
- 被控端 agent 端口：tailnet 与本地均为 `21416`；sidecar 负责 `100.x:21416 → 127.0.0.1:21416`。
- PC 控制端通过 sidecar 的 SOCKS5 `127.0.0.1:1055` 访问 tailnet；sidecar 状态接口为 `127.0.0.1:1056`。
- 配对后台容器内监听 `8080`，公网只通过 Caddy 暴露 HTTPS。
- DERP 必须由客户端直连，不能放在 HTTP 代理/负载均衡之后。

## 文档

- [完整部署手册（DNS、防火墙、证书、备份、升级、加固）](../docs/DEPLOYMENT.md)
- [配对 API 与控制协议](../docs/API.md)
- [Headscale 初始化](headscale/README.md)
- [tsnet sidecar](../apps/tsnet-sidecar/README.md)

## 注意

- 构建 rv-server 的上下文是仓库根目录；如构建缓慢，建议在仓库根目录添加 `.dockerignore` 排除 `target/`（本目录不负责根目录文件）。
- `deploy/data/` 含数据库与密钥，权限应为 `0700`；备份时按 `docs/DEPLOYMENT.md` 的步骤保证 SQLite 一致性。
