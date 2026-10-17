# RustVNC 服务端部署手册

本文从一台全新的 Linux 服务器开始，完成 DNS、防火墙、Headscale、DERP、配对后台、Caddy TLS 与客户端接入。所有域名、密钥均为占位符。

---

## 1. 架构与数据流

```text
                     公网 HTTPS (443/tcp)
  ┌──────────────┐   ┌──────────────────────────────┐
  │ PC/Android   │──▶│ Caddy                        │
  │ 控制端       │   │  rvnc.example.com    → rv-server:8080
  └──────┬───────┘   │  headscale.example.com → headscale:8080
         │           └──────────────────────────────┘
         │ tailnet (WireGuard)
         │ 100.x.y.z:21416
  ┌──────▼───────┐        ┌──────────────────────────┐
  │ PC 被控端    │        │ 自建 DERP                │
  │ tsnet sidecar│◀──────▶│ derp.example.com:8443    │
  │   ↕ 127.0.0.1│  P2P 失败时加密中继              │
  │ Rust agent   │        │ 3478/udp (STUN)          │
  └──────────────┘        └──────────────────────────┘
```

- **业务数据**（屏幕帧、输入事件）只走 tailnet：优先 P2P UDP，失败时经 DERP 加密中继。`rv-server` 不转发业务数据。
- **控制面**：Headscale 负责节点注册、预授权密钥、节点状态；rv-server 负责设备码、密码校验、免密令牌与 Web 管理台。
- **网络内嵌**：PC 端由 tsnet sidecar 加入 tailnet；Android 端使用官方 Tailscale VpnService。

---

## 2. 准备工作

### 2.1 主机要求

- Linux（Ubuntu 22.04/24.04、Debian 12 等），x86_64/arm64。
- 建议 2 vCPU、2 GB 内存、20 GB 磁盘起步。
- 已安装 Docker Engine 24+ 与 Docker Compose v2（`docker compose version` 可用）。
- 公网固定 IP（或可正确解析的 IPv4/IPv6）。
- 系统时间同步（`timedatectl` / chrony）；令牌与预授权密钥对时间敏感。

### 2.2 域名与 DNS

需要三个域名（可以是同一主域的子域）：

| 域名 | 指向 | 用途 |
| --- | --- | --- |
| `rvnc.example.com` | 本服务器 | 配对 API + Web 管理台 |
| `headscale.example.com` | 本服务器 | Headscale 控制面（客户端 tsnet 连接） |
| `derp.example.com` | 本服务器或独立 DERP 主机 | 自建 DERP/STUN |

DNS 记录：

```text
rvnc.example.com        A    203.0.113.10
headscale.example.com   A    203.0.113.10
derp.example.com        A    203.0.113.10
# 有 IPv6 时同时添加 AAAA
```

注意：

- **不要**给 `headscale` 与 `derp` 开启 CDN/反向代理（Cloudflare 橙色云朵等），必须 DNS-only 直连。把 DERP 放在代理/负载均衡之后会导致中继异常。
- Caddy 使用 HTTP-01 自动签发证书，需要公网 80/tcp 可达；若 80 被运营商封锁，请改用 DNS-01（见 Caddy 文档）。
- 被控端注册响应中的 `control_url` 就是 `headscale.example.com`，必须从客户端网络可访问。

### 2.3 端口与防火墙

| 端口 | 协议 | 来源 | 用途 | 公网 |
| --- | --- | --- | --- | --- |
| 22 | TCP | 管理 IP | SSH | 建议限制来源 |
| 80 | TCP | 0.0.0.0/0 | Caddy ACME + HTTP→HTTPS | 是 |
| 443 | TCP | 0.0.0.0/0 | Caddy HTTPS | 是 |
| 443 | UDP | 0.0.0.0/0 | HTTP/3（可选） | 是 |
| 8443 | TCP | 0.0.0.0/0 | derper（本编排默认；可改为 443） | 是 |
| 3478 | UDP | 0.0.0.0/0 | derper STUN | 是 |
| 8080 | TCP | 仅容器网络 | Headscale / rv-server | **否** |
| 9090 | TCP | 仅容器/回环 | Headscale metrics | **否** |
| 50443 | TCP | 仅回环 | Headscale gRPC | **否** |
| 1055/1056 | TCP | 仅本机 | 客户端 sidecar SOCKS/状态 | **否** |
| 21416 | TCP | 仅 tailnet | 被控端 agent（由 sidecar 暴露） | 否 |

出站方向需允许全部（Tailscale 直连打洞使用随机 UDP 端口）。无需为 P2P 固定开放入站 UDP。

ufw 示例：

```bash
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow 22/tcp
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
sudo ufw allow 443/udp
sudo ufw allow 8443/tcp
sudo ufw allow 3478/udp
sudo ufw enable
```

> 若 `DERP_HTTPS_PORT` 改为 `443`，请把 `8443` 规则换成对应端口，并注意与 Caddy 的 443 冲突（见第 4 节）。

### 2.4 安装 Docker

```bash
curl -fsSL https://get.docker.com | sudo sh
sudo usermod -aG docker "$USER"
newgrp docker
docker compose version
```

---

## 3. 部署步骤

### 步骤 0：获取代码

```bash
git clone <你的仓库地址> rust-vnc
cd rust-vnc/deploy
```

### 步骤 1：配置环境变量

```bash
cp .env.example .env
chmod 600 .env

# rv-server 以 UID 10001 运行；先创建并授权数据目录，避免首次启动写入失败
mkdir -p data backup
sudo chown 10001:10001 data
chmod 700 data
```

编辑 `.env`：

```dotenv
BASE_DOMAIN=example.com
RVNC_DOMAIN=rvnc.example.com
HEADSCALE_DOMAIN=headscale.example.com
DERP_DOMAIN=derp.example.com
ACME_EMAIL=admin@example.com

DERP_HTTPS_PORT=8443
DERP_CERTMODE=manual
DERP_URL=https://derp.example.com:8443

# 生成方式: openssl rand -hex 32
RV_ADMIN_TOKEN=<64 位十六进制随机串>

# 稍后创建 Headscale 用户/API Key 后回填；首次请保持为空
HEADSCALE_USER=1
HEADSCALE_API_KEY=
```

**不要**把 `.env` 提交到仓库；管理员令牌建议至少 32 字节随机值。

### 步骤 2：配置 Headscale

编辑 `deploy/headscale/config.yaml`，至少修改：

```yaml
server_url: https://headscale.example.com

dns:
  base_domain: rvnc.internal   # 可改为你自己的内部域名，无需公网解析

derp:
  urls:
    - https://derp.example.com:8443
```

若使用 Headscale ≤0.24，把 `prefixes` 改为旧的 `ip_prefixes` 列表写法（文件内有注释）。

### 步骤 3：准备 derper 证书

默认 `DERP_CERTMODE=manual`，derper 从 `deploy/derp/certs/` 读取：

```text
derp.example.com.crt    # 完整证书链
derp.example.com.key    # 私钥
```

推荐用 DNS-01 方式签发，避免与 Caddy 争用 80/443：

```bash
# 示例：certbot + DNS 插件
sudo certbot certonly --dns-<provider> -d derp.example.com

sudo cp /etc/letsencrypt/live/derp.example.com/fullchain.pem \
        deploy/derp/certs/derp.example.com.crt
sudo cp /etc/letsencrypt/live/derp.example.com/privkey.pem \
        deploy/derp/certs/derp.example.com.key
sudo chmod 600 deploy/derp/certs/derp.example.com.key
```

详细说明见 `deploy/derp/certs/README.md`。

### 步骤 4：启动 Headscale

```bash
docker compose up -d headscale
docker compose logs -f headscale
```

确认日志无配置解析错误后，创建用户与 API Key：

```bash
docker compose exec headscale headscale users create rvnc
docker compose exec headscale headscale users list
# 输出示例: 1 | rvnc | ...

docker compose exec headscale headscale apikeys create --expiration 90d
# 输出示例: hskey-api-...
```

把用户 ID 与 API Key 回填到 `.env` 的 `HEADSCALE_USER` / `HEADSCALE_API_KEY`。

### 步骤 5：启动其余服务

```bash
docker compose up -d --build
docker compose ps
docker compose logs -f rv-server caddy derper
```

首次启动顺序：

1. Headscale 启动并监听内部 8080；
2. rv-server 构建（Rust release，首次可能数分钟）并连接 Headscale；
3. Caddy 为两个域名申请证书；
4. derper 使用 manual 证书启动。

### 步骤 6：验证

```bash
# 配对后台
curl -s https://rvnc.example.com/healthz
# {"ok":true,"api":"v1","version":"0.1.0"}

# 管理台登录
ADMIN_TOKEN='<deploy/.env 中的 RV_ADMIN_TOKEN>'
curl -s https://rvnc.example.com/api/admin/login \
  -H 'Content-Type: application/json' \
  -d "{\"token\":\"$ADMIN_TOKEN\"}"

# Headscale 控制面（应返回 200/404 而非证书错误）
curl -sI https://headscale.example.com/

# derper TLS（证书域名必须匹配）
curl -sI https://derp.example.com:8443/
```

浏览器打开 `https://rvnc.example.com/admin/`，输入管理员令牌，检查“设备总览”。

### 步骤 7：检查 Headscale 与 rv-server 的集成

```bash
# 在 rv-server 容器内没有 curl/wget 时，可从宿主机经公网域名调用
ADMIN_TOKEN='<管理员令牌>'
curl -s https://rvnc.example.com/api/admin/headscale/nodes \
  -H "Authorization: Bearer $ADMIN_TOKEN"
```

若返回 `400 Headscale 尚未配置`，说明数据库中没有 URL/API Key：检查 `.env` 后重启 rv-server（首次启动时若 Key 为空，后续通过管理台“设置”页面填写，或先填好再启动）。

> rv-server 启动时只在数据库缺少设置时写入环境变量；数据库已有值优先。

---

## 4. DERP 端口与证书专题

### 4.1 为什么默认使用 8443

DERP 必须由客户端**直连**，不能经过 HTTP 代理/负载均衡。同时 Caddy 需要占用 443 为配对后台与 Headscale 提供 TLS。单机部署时两者不能同时绑定宿主机 443，因此本编排：

- derper 容器内监听 `:443`；
- 宿主机发布为 `8443`；
- Headscale 与客户端使用 `https://derp.example.com:8443`。

`deploy/.env` 的 `DERP_URL` 与 `deploy/headscale/config.yaml` 的 `derp.urls` 必须一致。

### 4.2 需要标准 443 时

推荐把 derper 放到**独立公网 IP 或独立主机**：

```dotenv
DERP_HTTPS_PORT=443
DERP_CERTMODE=letsencrypt   # 或 manual
DERP_URL=https://derp.example.com
```

同时修改 Headscale 的 `derp.urls`，并在该主机放行 80/443（ACME 需要 80）与 3478/udp。不要把这个 derper 接在任何代理后面。

### 4.3 证书续期

manual 模式下证书由你的 ACME 客户端续期后，重启 derper 使其重新加载：

```bash
docker compose restart derper
```

derper 的缓存与状态保存在命名卷 `derper-data`。

### 4.4 STUN

`-stun` 让 derper 在 UDP 3478 提供 STUN，帮助客户端发现公网端口。UDP 3478 必须放行；TCP 3478 不是必需。

---

## 5. 客户端接入

### 5.1 PC 被控端

1. 启动 Rust 客户端，填写配对后台地址（`https://rvnc.example.com`）并注册设备。
2. 客户端调用 `POST /api/v1/devices/register`，得到：
   - `device_code`、`password`、`agent_secret`
   - `headscale.control_url`、`headscale.preauth_key`、`headscale.hostname`、`headscale.derp_url`
3. 确保 sidecar 与客户端一起启动，环境变量：

   ```powershell
   $env:TS_AUTHKEY   = $register.headscale.preauth_key
   $env:TS_HOSTNAME  = $register.headscale.hostname
   $env:TS_CONTROLURL = $register.headscale.control_url
   $env:RVNC_AGENT_ADDR = "127.0.0.1:21416"
   ```

4. sidecar 上线后，客户端读取 `http://127.0.0.1:1056/healthz` 的 `ip4`。
5. 客户端每 30 秒调用 `POST /api/v1/devices/heartbeat`，上报 `tailscale_ip` 与 `listen_port=21416`。
6. Rust agent 监听 `127.0.0.1:21416`；sidecar 把 tailnet `:21416` 转发到该地址。
7. 被控端在控制流 `Hello` 之后调用 `/api/v1/session/verify` 校验控制端令牌。

Windows 上无需为 1055/1056 开放防火墙入站规则（仅回环）。

### 5.2 PC 控制端

1. 控制端同样需要加入 tailnet。PC 设置页的“获取网络接入密钥”按钮会调用 `POST /api/v1/controllers/enroll` 自动签发（推荐）；也可以由管理员手动创建：

   ```bash
   docker compose exec headscale headscale preauthkeys create \
     --user 1 --reusable --expiration 24h
   ```

2. 以该密钥启动控制端 sidecar（hostname 可自定，例如 `rvnc-ctrl-laptop`）。
3. 控制端填写设备码 + 密码调用 `/api/v1/pair`，保存 `token` 与稳定的 `controller_id`。
4. 连接时调用 `/api/v1/connect`，得到 `tailscale_ip` 与 `port`。
5. Rust 控制端通过 `socks5://127.0.0.1:1055` 连接该地址（见 `crates/rv-net`），随后发送控制协议 `Hello`。
6. 后续连接只使用保存的令牌，无需再次输入密码；令牌可在管理台或被控端“已配对设备”中吊销。

### 5.3 Android 控制端

- 集成官方 Tailscale Android VpnService（不是本 sidecar），登录同一 Headscale tailnet。
- 配对流程与 PC 控制端一致：`/pair` → 保存令牌 → `/connect` → 连接 `100.x:21416`。
- 触摸/键盘事件按控制协议发送；画面严格保持被控端原始宽高比。
- 令牌存入 `EncryptedSharedPreferences`。

---

## 6. 备份与恢复

### 6.1 需要备份的内容

| 内容 | 位置 | 说明 |
| --- | --- | --- |
| rv-server 数据库 | `deploy/data/rvnc.db`（含 `-wal`/`-shm`） | 设备、密码哈希、令牌哈希、配对记录、Headscale API Key（明文） |
| Headscale 数据库 | 命名卷 `rustvnc_headscale-data` | 节点、用户、API Key、预授权密钥 |
| Headscale 配置 | `deploy/headscale/config.yaml` | 手工维护 |
| derper 证书/私钥 | `deploy/derp/certs/` | 私有密钥，单独加密备份 |
| `.env` | `deploy/.env` | 管理员令牌、Headscale API Key |
| Caddy 数据 | `deploy/data/caddy/` | 证书缓存（可重新签发，可选） |

### 6.2 在线备份 SQLite（推荐）

宿主机安装 `sqlite3` 后，可在服务运行时做一致性备份：

```bash
mkdir -p deploy/backup
sqlite3 deploy/data/rvnc.db ".backup 'deploy/backup/rvnc-$(date +%F-%H%M).db'"
```

### 6.3 停机备份（无 sqlite3 时）

```bash
cd deploy
docker compose stop rv-server
mkdir -p backup
cp -a data/rvnc.db data/rvnc.db-wal data/rvnc.db-shm backup/ 2>/dev/null || true
docker compose start rv-server
```

### 6.4 Headscale 备份

```bash
cd deploy
docker compose stop headscale
docker run --rm \
  -v rustvnc_headscale-data:/data \
  -v "$PWD/backup:/backup" \
  alpine tar czf /backup/headscale-$(date +%F-%H%M).tar.gz -C /data .
docker compose start headscale
```

### 6.5 恢复

```bash
cd deploy
docker compose down

# 恢复 rv-server
cp backup/rvnc-YYYY-MM-DD-HHMM.db data/rvnc.db
sudo chown 10001:10001 data/rvnc.db
sudo chmod 600 data/rvnc.db

# 恢复 Headscale 卷
docker run --rm \
  -v rustvnc_headscale-data:/data \
  -v "$PWD/backup:/backup" \
  alpine sh -c 'rm -rf /data/* && tar xzf /backup/headscale-YYYY-MM-DD-HHMM.tar.gz -C /data'

docker compose up -d
```

### 6.6 建议的定时任务

```cron
# 每天 03:20 备份 rv-server，保留 14 天
20 3 * * * cd /opt/rust-vnc/deploy && sqlite3 data/rvnc.db ".backup 'backup/rvnc-$(date +\%F).db'" && find backup -name 'rvnc-*.db' -mtime +14 -delete
```

---

## 7. 升级

升级前务必先备份数据库与 `.env`。

### 7.1 rv-server

```bash
cd rust-vnc
git pull
cd deploy
docker compose build rv-server
docker compose up -d rv-server
docker compose logs -f rv-server
```

- 数据库表使用 `CREATE TABLE IF NOT EXISTS` 的增量初始化；现有数据保留。
- 如修改了 `crates/rv-server` 的表结构，升级前仍需备份。

### 7.2 Headscale

```bash
cd deploy
# 1. 备份（见第 6 节）
# 2. 在 docker-compose.yml 中固定新版本，例如 headscale/headscale:0.27.0
docker compose pull headscale
docker compose up -d headscale
docker compose logs -f headscale
```

Headscale 不同版本的配置键可能变化（如 `ip_prefixes` → `prefixes`），升级前阅读对应 release notes。

### 7.3 derper

修改 `docker-compose.yml` 中构建参数或 `deploy/derp/Dockerfile` 的 `DERP_VERSION`，然后：

```bash
docker compose build --no-cache derper
docker compose up -d derper
```

建议让 derper 版本与客户端 `tailscale.com` 版本接近。

### 7.4 客户端 sidecar

```bash
cd apps/tsnet-sidecar
go get tailscale.com@<version>
go mod tidy
go build -o rvnc-tsnet .
```

升级后重启桌面端，确认 `/healthz` 正常、控制会话可建立。

---

## 8. 安全加固清单

- [ ] **管理员令牌**：使用 `openssl rand -hex 32` 生成；至少 32 字符；仅通过 `.env` 注入；定期轮换。
- [ ] **限制管理面**：`/admin` 与 `/api/admin/*` 不应直接暴露给所有人。可通过 Caddy `remote_ip` 白名单、云防火墙、VPN 或内网访问；`/api/admin/login` 没有独立限流。
- [ ] **`.env` 权限**：`chmod 600`，不要提交仓库，不要出现在日志中。
- [ ] **数据目录**：`chmod 700 deploy/data`；`rvnc.db` 属主 `10001:10001`、权限 `600`。数据库中的 `headscale_api_key` 为明文。
- [ ] **禁止直接暴露 8080/9090/50443**：只允许 Caddy 访问 rv-server 与 Headscale。
- [ ] **`RV_TRUST_PROXY=1` 的前提**：8080 只可经 Caddy 访问，否则攻击者可伪造 `X-Forwarded-For` 绕过按 IP 限流。本仓库的 Caddyfile 会用 `{remote_host}` **覆盖**该头；如果你在前面又加了 CDN/代理，需要显式配置 Caddy 的 `trusted_proxies` 或改用可信头。
- [ ] **TLS**：由 Caddy 自动签发与续期；开启 HSTS；不要提供 HTTP-only 入口。Headscale 的 `server_url` 必须是 HTTPS。
- [ ] **DERP**：绝不放到 HTTP 代理/负载均衡之后；及时更新 derper；限制 derper 状态目录权限。
- [ ] **Headscale API Key**：90 天轮换；泄露后立即 `apikeys expire` 并在管理台更新。
- [ ] **限流**：当前为进程内滑动窗口，重启清空；不要多实例共享同一数据库而不做外部限流。设备码/密码已启用按设备码与按 IP 的限制。
- [ ] **令牌生命周期**：默认 30 天；通过管理台调整 `token_ttl_days`；离职/换机时吊销令牌或删除设备。
- [ ] **最小权限运行**：rv-server 以 UID 10001 运行；容器不使用 `privileged`；只挂载必要目录。
- [ ] **主机加固**：SSH 密钥登录、禁用密码、fail2ban、系统与 Docker 定期更新、时间同步。
- [ ] **客户端**：sidecar 的 SOCKS5（1055）与状态接口（1056）必须保持回环；控制端令牌使用系统密钥环/EncryptedSharedPreferences；预授权密钥一次性使用，不要写入日志。
- [ ] **可选：Headscale ACL**：用 policy 限制只有控制端节点能访问被控端的 `tcp:21416`，并限制谁可访问配对后台。启用前备份数据库。

---

## 9. 排障

### 9.1 `https://rvnc.example.com/healthz` 返回 502

```bash
docker compose ps
docker compose logs --tail=200 caddy rv-server
```

- rv-server 构建失败：确认构建上下文是仓库根目录，且 `Cargo.lock` 存在。
- 容器启动即退出：检查 `RV_ADMIN_TOKEN` 是否已设置（compose 使用 `${RV_ADMIN_TOKEN:?...}` 强制要求）。
- Caddy 证书失败：检查 DNS、80/tcp、域名是否被 CDN 代理。

### 9.2 注册响应中 `headscale` 为 `null`

- 检查 `.env` 的 `HEADSCALE_API_KEY` 与 `HEADSCALE_USER`。
- 查看 `docker compose logs rv-server` 中的 Headscale 错误。
- 验证：

  ```bash
  docker compose exec headscale headscale users list
  docker compose exec headscale headscale apikeys list
  ```

- 若设置是在 rv-server 首次启动后才写入 `.env`，重启 rv-server 使 seed 生效；若数据库中已有空值/旧值，请通过管理台设置页更新。

### 9.3 `/api/v1/connect` 返回 409 `device_offline`

1. 被控端 sidecar 是否运行：`curl http://127.0.0.1:1056/healthz`（在被控端本机）。
2. `online` 是否为 `true`、`ip4` 是否为 `100.x`。
3. 客户端是否每 30 秒发送心跳（超过 90 秒会离线）。
4. 节点名是否为 `rvnc-<code 小写>`：`docker compose exec headscale headscale nodes list`。
5. Headscale 中节点是否在线、IP 是否已分配。

### 9.4 配对始终返回 401

- 设备码是否按 6 位字母表输入（无 `0/O/1/I/L`）。
- 密码是否为注册响应中的当前密码；是否执行过修改密码。
- 是否触发 429（检查响应头 `Retry-After`），等待锁定结束。
- 通过管理台“配对记录”查看 `reason`（`unknown_device` / `bad_password` / `invalid_code`）。

### 9.5 能连上但走中继 / 延迟高

- 在客户端主机运行 `tailscale netcheck`（如已安装）或查看 sidecar 日志中的 DERP 区域。
- 确认 UDP 出站未被防火墙阻断；DERP 的 UDP 3478 已放行。
- 确认 Headscale 的 `derp.urls` 指向正确的 derper，且 derper 证书域名匹配。
- 若始终走中继，检查双方 NAT 类型；对称 NAT 下 P2P 失败属预期，DERP 中继仍为加密流量。

### 9.6 sidecar 启动失败

- `TS_AUTHKEY is required`：被控端必须使用注册响应中的预授权密钥。
- `join tailnet: ...`：检查 `TS_CONTROLURL`、DNS、Headscale 证书与 443 连通性。
- `RVNC_STATUS_ADDR ... must bind a loopback address`：状态接口只允许回环，修改为 `127.0.0.1:1056`。
- 端口占用：确认 `21416`、`1055`、`1056` 未被其他进程占用。

### 9.7 数据库权限错误

rv-server 以 UID 10001 运行，bind mount 目录需要可写：

```bash
sudo chown -R 10001:10001 deploy/data
sudo chmod 700 deploy/data
```

### 9.8 管理台设置改了但未生效

- 启动时的环境变量只做 seed-if-missing；数据库已有值时以数据库为准。
- Headscale API Key 通过管理台更新后无需重启；`headscale_url` 等同样即时生效。
- 若数据库损坏或需要重置，请先备份再删除 `deploy/data/rvnc.db`（会丢失设备、令牌与配对记录）。
