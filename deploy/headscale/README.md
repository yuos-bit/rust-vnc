# Headscale 初始化与管理

本目录的 `config.yaml` 由 `docker-compose.yml` 只读挂载到 Headscale 容器。首次部署请先修改其中的：

- `server_url`：必须是客户端可访问的 HTTPS 地址，与 `HEADSCALE_DOMAIN` 一致；
- `derp.urls`：必须指向已部署的 derper，并带上对外端口；
- `dns.base_domain`：MagicDNS 域名（无需公网解析）。

## 1. 启动 Headscale

```bash
cd deploy
docker compose up -d headscale
docker compose logs -f headscale
```

客户端（tsnet）会通过 Caddy 访问 `https://headscale.example.com`；容器内部的 REST API 地址是 `http://headscale:8080`，供 rv-server 调用。

## 2. 创建用户

```bash
docker compose exec headscale headscale users create rvnc
```

查看用户 ID：

```bash
docker compose exec headscale headscale users list
```

输出中的 ID（例如 `1`）就是 `deploy/.env` 里的 `HEADSCALE_USER`。rv-server 创建预授权密钥时会把它作为 `user` 字段提交。

> 部分较新版本允许直接使用用户名；为兼容性，本项目文档与默认配置使用数字 ID。

## 3. 创建 API Key

```bash
docker compose exec headscale headscale apikeys create --expiration 90d
```

把输出的密钥填入 `deploy/.env` 的 `HEADSCALE_API_KEY`。该密钥：

- 只有 rv-server 使用，**绝不返回给客户端**；
- 会以明文存储在 rv-server 的 SQLite `settings` 表中（`headscale_api_key`），因此必须保护 `deploy/data/` 权限；
- 建议每 90 天轮换一次。轮换后重启 rv-server，或通过 Web 管理台“设置”更新（数据库中已有值不会被环境变量覆盖）。

验证 API：

```bash
docker compose exec rv-server \
  wget -qO- --header="Authorization: Bearer $HEADSCALE_API_KEY" \
  http://headscale:8080/api/v1/node
```

（rv-server 运行镜像可能没有 `wget`；也可从宿主机通过 Caddy 的 Headscale 域名调用，或用 curl 进入任意容器测试。）

## 4. 预授权密钥（Pre-Auth Key）

被控端注册时，rv-server 自动调用：

```http
POST /api/v1/preauthkey
Authorization: Bearer <HEADSCALE_API_KEY>
Content-Type: application/json

{
  "user": "1",
  "reusable": false,
  "ephemeral": false,
  "aclTags": [],
  "hostname": "rvnc-a2b3c4"
}
```

返回的 `preAuthKey.key` 写入注册响应的 `headscale.preauth_key`，客户端把它作为 `TS_AUTHKEY` 启动 sidecar。hostname 规则固定为 `rvnc-<设备码小写>`，rv-server 之后按该名字解析节点 IP。

响应结构（不同 Headscale 版本字段可能略有差异）：

```json
{
  "preAuthKey": {
    "user": { "id": "1", "name": "rvnc" },
    "id": "1",
    "key": "hskey-auth-...",
    "reusable": false,
    "ephemeral": false,
    "used": false,
    "expiration": "2026-10-18T12:00:00Z",
    "createdAt": "2026-10-17T12:00:00Z",
    "aclTags": []
  }
}
```

> rv-server 的请求未显式指定 expiration，使用 Headscale 的默认有效期。若一次性密钥过期，被控端需要重新注册（或在管理台删除设备后重新注册）。

### 为 PC 控制端手动创建密钥

配对后台目前只为“被控端注册”签发密钥。PC 控制端如也需要加入 tailnet，可由管理员手动创建：

```bash
docker compose exec headscale headscale preauthkeys create \
  --user 1 --reusable --expiration 24h
```

将输出作为控制端 sidecar 的 `TS_AUTHKEY`，hostname 可自由设置（例如 `rvnc-ctrl-<用户名>`）。

## 5. 节点审批

使用预授权密钥加入的节点通常会自动批准。如果启用了手动审批：

```bash
docker compose exec headscale headscale nodes list
docker compose exec headscale headscale nodes approve --identifier <节点ID>
```

确认节点名与 IP：

```bash
docker compose exec headscale headscale nodes list
```

被控端节点名应为 `rvnc-<code>`，地址应落在 `100.64.0.0/10`。

## 6. 常用运维命令

```bash
# 节点列表 / 详情
docker compose exec headscale headscale nodes list

# 删除某个节点（设备回收/重装）
docker compose exec headscale headscale nodes delete --identifier <节点ID>

# 列出 API Key
docker compose exec headscale headscale apikeys list

# 吊销 API Key（新版本按前缀操作）
docker compose exec headscale headscale apikeys expire --prefix "<PREFIX>"

# 备份数据库（先停服务，保证 SQLite 一致）
docker compose stop headscale
docker run --rm -v rustvnc_headscale-data:/data -v "$PWD/backup:/backup" \
  alpine tar czf /backup/headscale-$(date +%F).tar.gz -C /data .
docker compose start headscale
```

## 7. 与 rv-server 的集成点

| 用途 | Headscale API | rv-server 调用位置 |
| --- | --- | --- |
| 设备注册时签发一次性密钥 | `POST /api/v1/preauthkey` | `crates/rv-server/src/headscale.rs::create_preauth_key` |
| 查询节点 IP / 在线状态 | `GET /api/v1/node` | `list_nodes` / `resolve_node_ip`，以及管理台“Headscale 节点” |

`HEADSCALE_URL` 使用容器网络地址 `http://headscale:8080`；`HEADSCALE_CONTROL_URL` 使用公网 HTTPS 地址，二者不能混淆。公网地址写入注册响应，供客户端 sidecar 的 `TS_CONTROLURL` 使用。

## 8. 排错

- **注册响应 `headscale` 为 `null`**：rv-server 没有配置 URL/API Key，或创建预授权密钥失败。查看 `docker compose logs rv-server`。
- **rv-server 返回 502 `upstream_error`**：Headscale 不可达或 API Key 无效；检查 `HEADSCALE_URL`、Key 与 Headscale 日志。
- **节点上线但 rv-server 查不到 IP**：确认节点名严格为 `rvnc-<code 小写>`；Headscale 的 `givenName` 与节点名不一致时可查看 `headscale nodes list -o json`。
- **DERP 不生效**：确认 `derp.urls` 与客户端配置一致、derper 证书域名匹配、TCP 8443/443 与 UDP 3478 放行；绝不能让 derper 位于 HTTP 代理之后。
