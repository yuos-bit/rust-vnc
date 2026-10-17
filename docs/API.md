# RustVNC 配对 API 与控制协议

本文档对应 `crates/rv-server/src/main.rs`、`api.rs`、`admin.rs`、`web.rs` 与 `crates/rv-proto`。

- 基础地址（公网）：`https://rvnc.example.com`
- API 版本：`v1`（`GET /healthz` 的 `api` 字段）
- 请求与响应：除静态资源外均为 `application/json`
- 时间：所有 `*_at`、`server_time`、`expires_at` 均为 **Unix 秒**
- 错误响应统一为：

```json
{
  "error": {
    "code": "unauthorized",
    "message": "设备码或密码不正确"
  }
}
```

> 例外：JSON 解析失败、缺少 `Content-Type: application/json` 等发生在进入处理函数之前的 Axum 拒绝，会返回 400/415/422 的纯文本响应，不带上述信封。

## 1. 认证模型

| 身份 | 凭据 | 使用位置 |
| --- | --- | --- |
| 被控端（设备） | `device_code` + `agent_secret`（`agt_...`） | 心跳、会话校验、吊销令牌、修改密码 |
| 控制端 | `device_code` + `token`（`rvt_...`）+ 稳定的 `controller_id` | 建立连接、控制协议 Hello 后的会话校验 |
| 管理员 | `Authorization: Bearer <RV_ADMIN_TOKEN>` | 所有 `/api/admin/*`（`/login` 除外） |

- `agent_secret` 只在设备注册响应中出现一次；服务端只保存 SHA-256。
- 控制端令牌默认有效期 30 天（可通过管理台设置 1–3650 天）；服务端只保存 SHA-256。
- 管理员令牌通过环境变量 `RV_ADMIN_TOKEN`（或旧名 `ADMIN_TOKEN`）设置；未设置时服务启动会随机生成并打印到 stdout。
- 管理员令牌比较使用常量时间比较；普通 API 没有 CORS 头，管理台与 API 同源。

## 2. 错误码与限流

| HTTP | `error.code` | 含义 |
| --- | --- | --- |
| 400 | `bad_request` | 参数格式/范围错误 |
| 401 | `unauthorized` | 凭据无效、令牌无效/过期/吊销、缺少管理员令牌 |
| 404 | `not_found` | 设备/令牌不存在，或令牌已吊销 |
| 409 | `device_offline` | 被控端未加入 tailnet 或已离线 |
| 429 | `rate_limited` | 触发失败次数限制；响应头 `Retry-After` 为秒数（至少 1） |
| 500 | `internal` | 服务器内部错误（详情只写日志，不返回客户端） |
| 502 | `upstream_error` | 调用 Headscale 失败 |

### 限流规则（内存滑动窗口）

| 场景 | 键 | 阈值 | 锁定时长 |
| --- | --- | --- | --- |
| 配对失败 | 设备码 | 5 次 / 10 分钟 | 5 分钟 |
| 配对失败 | 来源 IP | 15 次 / 10 分钟 | 10 分钟 |
| 配对失败 | 设备码 + IP | 同上（共享 IP 限制器） | 10 分钟 |
| 设备注册 | 来源 IP | 30 次 / 60 分钟 | 30 分钟 |

注意：

- 注册限制统计的是**尝试次数**，注册成功同样消耗一次配额。
- 配对成功会清除“设备码”和“设备码+IP”的失败计数。
- 限流在进程内保存，重启即清空；多实例部署时各自独立。
- 位于 Caddy 之后必须设置 `RV_TRUST_PROXY=1`，且不得直接暴露 8080，否则 `X-Forwarded-For` 可被伪造。
- 不存在的设备码也会执行一次等价的密码哈希校验，避免通过响应时间枚举设备码。

### 反枚举行为

配对失败（设备码不存在、密码错误、格式非法）统一返回：

```http
HTTP/1.1 401 Unauthorized
Content-Type: application/json

{"error":{"code":"unauthorized","message":"设备码或密码不正确"}}
```

服务端不会透露“设备码是否存在”。

## 3. 公共 API

### 3.1 `GET /healthz`

无需认证。

```json
{"ok":true,"api":"v1","version":"0.1.0"}
```

### 3.2 `GET /api/v1/info`

无需认证。`headscale_configured` 表示数据库中是否同时存在 Headscale URL 与 API Key。

```json
{
  "version": "0.1.0",
  "protocol_version": 1,
  "headscale_configured": true
}
```

### 3.3 `POST /api/v1/devices/register`

被控端首次启动时注册。无需认证，受每 IP 注册限流。

请求：

```json
{
  "name": "My Desktop",
  "platform": "windows",
  "fingerprint": "sha256:9f2c...",
  "version": "0.1.0"
}
```

字段规则：

| 字段 | 必填 | 规则 |
| --- | --- | --- |
| `name` | 是 | 去空白/控制字符后非空，最长 64 字符 |
| `platform` | 否 | 最长 32；可省略或为空字符串 |
| `fingerprint` | 否 | 最长 128 |
| `version` | 否 | 最长 32 |

响应 `200`：

```json
{
  "device_id": "dev_6f1c2a...",
  "device_code": "A2B3C4",
  "password": "K7M2QX9P",
  "agent_secret": "agt_3Jk...",
  "protocol_version": 1,
  "headscale": {
    "control_url": "https://headscale.example.com",
    "preauth_key": "hskey-auth-...",
    "hostname": "rvnc-a2b3c4",
    "derp_url": "https://derp.example.com:8443"
  }
}
```

- `password`、`agent_secret`、`preauth_key` **只返回这一次**，客户端必须安全保存。
- `headscale` 可能为 `null`：后台未配置 Headscale，或签发预授权密钥失败。此时注册仍成功，但设备暂时无法加入 tailnet。
- `hostname` 固定为 `rvnc-<device_code 小写>`，必须原样传给 sidecar 的 `TS_HOSTNAME`。
- 设备码字母表为 `23456789ABCDEFGHJKLMNPQRSTUVWXYZ`（无 `0/O/1/I/L`），共 6 位。
- 默认密码为 8 位同字母表随机串。

错误：

- `400 bad_request`：`name` 去空白后为空。
- `429 rate_limited`：注册过于频繁。
- `500 internal`：无法生成唯一设备码或数据库错误。

### 3.4 `POST /api/v1/devices/heartbeat`

被控端认证后上报在线状态。建议每 30 秒一次（超过 90 秒未上报会被标记离线）。

请求：

```json
{
  "device_code": "A2B3C4",
  "agent_secret": "agt_3Jk...",
  "tailscale_ip": "100.64.0.7",
  "listen_port": 21416,
  "version": "0.1.0"
}
```

响应 `200`：

```json
{
  "online": true,
  "server_time": 1760000000,
  "paired_controllers": [
    {
      "token_id": "tok_9ab...",
      "controller_id": "ctrl_phone_1",
      "name": "My Phone",
      "platform": "android",
      "paired_at": 1759999000,
      "last_seen": 1760000000
    }
  ]
}
```

行为：

- 更新 `online=1`、`last_seen`、`tailscale_ip`、`listen_port`。
- `tailscale_ip` 必须是合法 IP 字符串，否则 `400 bad_request`。
- 非 `100.64.0.0/10` 地址仍会保存，但服务端会记录警告。
- 若请求未提供 IP 且已配置 Headscale，服务端会尝试在 4 秒内按 hostname 解析节点 IP。
- 返回当前未吊销且未过期的控制端令牌列表，供被控端展示“已配对设备”。

错误：`401 unauthorized`（设备不存在或 `agent_secret` 不匹配）、`400 bad_request`、`429` 不适用。

### 3.5 `POST /api/v1/pair`

控制端使用设备码 + 密码换取免密令牌。

请求：

```json
{
  "device_code": "A2B3C4",
  "password": "K7M2QX9P",
  "controller_id": "ctrl_phone_1",
  "controller_name": "My Phone",
  "platform": "android"
}
```

- `device_code` 会去空白、去掉 `-`/`_` 并转大写，然后校验 6 位字母表。
- `controller_id` 建议由控制端生成稳定 UUID 并持久化；后续 `/connect` 必须一致。
- `controller_name` 最长 64；`platform` 最长 32。

响应 `200`：

```json
{
  "token": "rvt_5Hc...",
  "token_id": "tok_9ab...",
  "device_code": "A2B3C4",
  "device_name": "My Desktop",
  "tailscale_ip": "100.64.0.7",
  "online": true,
  "server_version": "0.1.0"
}
```

- `token` 只在本次响应返回；控制端应存入系统密钥环 / EncryptedSharedPreferences。
- `tailscale_ip` 可能为 `null`（被控端尚未上报或解析到地址）。
- `online` 基于最近 90 秒心跳。

错误：`401 unauthorized`（统一文案）、`429 rate_limited`。

### 3.6 `POST /api/v1/connect`

控制端用令牌获取被控端 tailnet 地址与端口。

请求：

```json
{
  "device_code": "A2B3C4",
  "token": "rvt_5Hc...",
  "controller_id": "ctrl_phone_1"
}
```

响应 `200`：

```json
{
  "device_code": "A2B3C4",
  "device_name": "My Desktop",
  "tailscale_ip": "100.64.0.7",
  "port": 21416,
  "online": true
}
```

行为与错误：

- 令牌必须属于该设备、未吊销、未过期，且 `controller_id` 与配对时一致。
- 成功会更新令牌 `last_used`。
- `401 unauthorized`：设备码/令牌无效、令牌过期或吊销、令牌不属于该控制端、设备不存在。
- `409 device_offline`：
  - 设备尚未分配 tailnet IP：

    ```json
    {"error":{"code":"device_offline","message":"被控端尚未加入虚拟网络，请确认其网络层已启动"}}
    ```

  - 设备离线：

    ```json
    {"error":{"code":"device_offline","message":"被控端离线，请确认设备已开机并运行 RustVNC"}}
    ```

- 此接口本身没有额外限流；不要用它暴力枚举令牌。

### 3.7 `POST /api/v1/session/verify`

被控端在控制流 `Hello` 之后调用，校验控制端令牌。需要设备认证（`agent_secret`），令牌明文不会离开被控端与本服务之间的请求。

请求：

```json
{
  "device_code": "A2B3C4",
  "agent_secret": "agt_3Jk...",
  "token": "rvt_5Hc...",
  "controller_id": "ctrl_phone_1"
}
```

有效时 `200`：

```json
{
  "valid": true,
  "token_id": "tok_9ab...",
  "controller_name": "My Phone",
  "expires_at": 1762595200
}
```

无效时仍是 `200`（便于被控端处理业务错误）：

```json
{
  "valid": false,
  "token_id": null,
  "controller_name": null,
  "expires_at": null
}
```

错误：`401 unauthorized` 表示设备自身认证失败（不是令牌无效）。
成功校验会更新令牌 `last_used`。

### 3.8 `POST /api/v1/agent/tokens/revoke`

被控端解除与某个控制端的配对。需要设备认证。

请求：

```json
{
  "device_code": "A2B3C4",
  "agent_secret": "agt_3Jk...",
  "token_id": "tok_9ab..."
}
```

响应 `200`：

```json
{"ok": true}
```

错误：

- `401 unauthorized`：设备认证失败。
- `404 not_found`：令牌不存在、不属于该设备或已被吊销。

### 3.9 `POST /api/v1/agent/password`

被控端轮换配对密码。需要设备认证。已配对的令牌不受影响。

请求（生成新密码）：

```json
{
  "device_code": "A2B3C4",
  "agent_secret": "agt_3Jk...",
  "new_password": null
}
```

请求（指定密码）：

```json
{
  "device_code": "A2B3C4",
  "agent_secret": "agt_3Jk...",
  "new_password": "correct-horse-battery"
}
```

响应 `200`：

```json
{"password": "N4W8RT2Y"}
```

- 省略 `new_password` 或传 `null` 时服务端生成 8 位随机密码。
- 指定密码长度必须为 8–64 个字符且不含控制字符，否则 `400 bad_request`。
- 错误：`401 unauthorized`。

### 3.10 `POST /api/v1/controllers/enroll`

为**控制端**（手机或独立 PC 控制端）签发一次性 tailnet 预授权密钥，用于其内嵌
Tailscale/tsnet 网络层加入同一张网。服务端不会保存密钥，只返回一次。

请求：

```json
{
  "name": "Alice-Laptop",
  "platform": "windows",
  "fingerprint": "DESKTOP-ABC",
  "controller_id": "733b932ffa882def6ff936cb880a5f33"
}
```

响应 `200`：

```json
{
  "preauth_key": "hskey-auth-...",
  "control_url": "https://headscale.example.com",
  "hostname": "rvnc-ctrl-3f9a2c1b7d",
  "derp_url": "https://derp.example.com:8443"
}
```

- 需要服务端已配置 `HEADSCALE_URL` + `HEADSCALE_API_KEY`；未配置返回
  `400 bad_request`。
- 受来源 IP 的注册限流保护（30 次/小时）；调用 Headscale 失败返回
  `502 upstream_error`。
- PC 设置页“获取网络接入密钥”和 Android 设置页“自动获取网络接入密钥”均调用此接口。
- 密钥只显示一次；客户端应使用系统密钥环 / EncryptedSharedPreferences 保存，
  不要写入日志。

## 4. 管理 API

除 `POST /api/admin/login` 外，所有路由都要求：

```http
Authorization: Bearer <RV_ADMIN_TOKEN>
```

缺失或错误统一返回 `401 unauthorized`。

### 4.1 `POST /api/admin/login`

请求：

```json
{"token": "your-admin-token"}
```

成功：

```json
{"ok": true}
```

失败：`401 {"error":{"code":"unauthorized","message":"管理员令牌无效"}}`。

> 该接口没有独立的速率限制，安全性依赖令牌长度与网络访问控制。请使用至少 32 字节随机令牌，并限制管理端来源。

### 4.2 `GET /api/admin/stats`

```json
{
  "devices_total": 3,
  "devices_online": 1,
  "active_tokens": 2,
  "pairings_today": 5
}
```

- `devices_online`：`last_seen >= now - 90s`。
- `active_tokens`：未吊销且未过期。
- `pairings_today`：UTC 当日成功配对次数（按 Unix 秒向下取整到 86400 的倍数）。

### 4.3 `GET /api/admin/devices`

返回数组，按创建时间倒序：

```json
[
  {
    "id": "dev_6f1c2a...",
    "code": "A2B3C4",
    "name": "My Desktop",
    "platform": "windows",
    "tailscale_ip": "100.64.0.7",
    "online": true,
    "last_seen": 1760000000,
    "created_at": 1759990000,
    "paired_controllers": 2,
    "active_tokens": 2
  }
]
```

`paired_controllers` 为去重后的有效控制端数量；`active_tokens` 为有效令牌数量。

### 4.4 `DELETE /api/admin/devices/{code}`

- `{code}` 会按设备码规则归一化。
- 成功：`200 {"ok":true}`。
- `400 bad_request`：设备码格式错误。
- `404 not_found`：设备不存在。
- 删除设备会级联删除其令牌；配对历史保留。

### 4.5 `GET /api/admin/tokens`

返回数组，按创建时间倒序（包含已吊销/过期记录）：

```json
[
  {
    "id": "tok_9ab...",
    "device_code": "A2B3C4",
    "device_name": "My Desktop",
    "controller_id": "ctrl_phone_1",
    "controller_name": "My Phone",
    "platform": "android",
    "created_at": 1759999000,
    "expires_at": 1762591000,
    "revoked_at": null,
    "last_used": 1760000000
  }
]
```

### 4.6 `DELETE /api/admin/tokens/{id}`

- 成功：`200 {"ok":true}`。
- `404 not_found`：令牌不存在或已经吊销。
- 吊销立即生效：`/connect` 与 `/session/verify` 都会拒绝该令牌。

### 4.7 `GET /api/admin/pairings?limit=100`

查询参数：

- `limit`：默认 `100`，服务端限制在 `1–500`。

返回数组，按记录 ID 倒序：

```json
[
  {
    "id": 42,
    "device_code": "A2B3C4",
    "controller_name": "My Phone",
    "ip": "203.0.113.10",
    "success": true,
    "reason": null,
    "created_at": 1760000000
  }
]
```

失败原因 `reason` 可能为：`invalid_code`、`unknown_device`、`bad_password`。

### 4.8 `GET /api/admin/settings`

```json
{
  "headscale_url": "http://headscale:8080",
  "headscale_user": "1",
  "headscale_api_key_set": true,
  "derp_url": "https://derp.example.com:8443",
  "public_url": "https://rvnc.example.com",
  "agent_port": 21416,
  "token_ttl_days": 30,
  "updated_at": 1760000000
}
```

**不会返回 API Key 明文**，只返回是否已设置。

### 4.9 `PUT /api/admin/settings`

所有字段可选，只更新出现的字段：

```json
{
  "headscale_url": "http://headscale:8080",
  "headscale_api_key": "new-api-key",
  "headscale_user": "1",
  "derp_url": "https://derp.example.com:8443",
  "public_url": "https://rvnc.example.com",
  "agent_port": 21416,
  "token_ttl_days": 30
}
```

校验：

- `headscale_url`：空或必须以 `http://` / `https://` 开头；末尾 `/` 会被去掉。
- `headscale_api_key`：空字符串表示清除；否则按原样保存。
- `agent_port`：1–65535。
- `token_ttl_days`：1–3650。

响应为更新后的 `SettingsView`（同 4.8）。

> 启动时的环境变量只会在数据库缺少对应设置时写入（seed-if-missing）。数据库中的值优先，因此修改 `.env` 后若设置已存在，需要重启无效，请通过本接口更新。

### 4.10 `GET /api/admin/headscale/nodes`

调用 Headscale `GET /api/v1/node`，返回：

```json
[
  {
    "id": "1",
    "name": "rvnc-a2b3c4",
    "ip_addresses": ["100.64.0.7", "fd7a:115c:a1e0::1"],
    "online": true,
    "last_seen": "2026-10-17T12:00:00Z"
  }
]
```

错误：

- `400 bad_request`：Headscale 未配置（缺少 URL 或 API Key）。
- `502 upstream_error`：Headscale 请求失败（8 秒超时、非 2xx 或响应格式错误）。

## 5. Web 管理台静态路由

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/` | `308` 永久重定向到 `/admin/` |
| `GET` | `/admin`、`/admin/` | 管理台 HTML（公开静态文件，数据接口仍需 Bearer） |
| `GET` | `/admin/styles.css` | 样式 |
| `GET` | `/admin/app.js` | 前端脚本 |
| `GET` | `/admin/favicon.svg` | 图标 |

## 6. 远程控制协议（TCP，非 WebSocket）

`rv-server` **不提供 WebSocket 端点**。屏幕帧与输入事件走独立 TCP 连接，经 Tailscale/WireGuard 加密，由 sidecar 转发到被控端 agent（默认 21416）。

### 6.1 连接建立

1. 控制端调用 `/api/v1/connect`，得到 `tailscale_ip` 与 `port`。
2. 控制端通过自己的 tsnet sidecar SOCKS5（`127.0.0.1:1055`）或 Android VpnService 连接 `tailscale_ip:port`。
3. 控制端发送 `ClientMessage::Hello`。
4. 被控端校验 `magic`/`wire_version`，并用自身 `agent_secret` 调用 `/api/v1/session/verify` 校验 `token` 与 `controller_id`。
5. 被控端返回 `ServerMessage::Hello { accepted, ... }` 或 `ServerMessage::Error`。
6. 随后：被控端持续发送 `Video`，控制端发送 `Input`、`Ping`，任一方可发 `Bye`。

### 6.2 帧格式

```text
+----------------------+-----------------------------------+
| u32 little-endian    | bincode(ClientMessage/ServerMessage) |
| body length (N)      | N bytes                           |
+----------------------+-----------------------------------+
```

- 序列化：`bincode` 2.x，`bincode::config::standard()`（变长整数编码，非自描述）。
- 最大帧：`MAX_FRAME_BYTES = 16 MiB`。接收方必须在分配内存前检查长度。
- `MAGIC = "RVNC"`，`WIRE_VERSION = 1`；版本不兼容应拒绝会话。
- 跨语言实现（如 Android JNI）应以 `crates/rv-proto/src/wire.rs` 为唯一事实来源，并锁定 `wire_version`。

### 6.3 `ClientMessage`

按枚举声明顺序（bincode 变体索引）：

| 索引 | 变体 | 字段 |
| --- | --- | --- |
| 0 | `Hello` | `magic: [u8;4]`, `wire_version: u16`, `device_code`, `token`, `controller_id`, `controller_name`, `platform` |
| 1 | `Input` | `InputEvent` |
| 2 | `Ping` | `id: u64` |
| 3 | `Bye` | 无 |

`InputEvent`：

| 索引 | 变体 | 字段 |
| --- | --- | --- |
| 0 | `MouseMove` | `x: f64`, `y: f64`（归一化坐标） |
| 1 | `MouseButton` | `button: MouseButton`, `pressed: bool` |
| 2 | `Wheel` | `delta_x: f64`, `delta_y: f64` |
| 3 | `Key` | `code: String`, `pressed: bool` |
| 4 | `Text` | `text: String`（IME/Unicode） |
| 5 | `Clipboard` | `text: String` |

`MouseButton`：`Left=0`、`Right=1`、`Middle=2`、`Back=3`、`Forward=4`。

### 6.4 `ServerMessage`

| 索引 | 变体 | 字段 |
| --- | --- | --- |
| 0 | `Hello` | `wire_version`, `accepted`, `session_id`, `device_name`, `width`, `height`, `fps` |
| 1 | `Video` | `seq`, `width`, `height`, `timestamp_ms`, `format`, `data` |
| 2 | `Pong` | `id: u64` |
| 3 | `Error` | `code`, `message` |

- `FrameFormat::Jpeg = 0`（当前唯一格式）。
- 控制端必须按 `width/height` **等比缩放**，允许黑边，禁止非等比拉伸。
- 输入坐标以 `0.0–1.0` 归一化，被控端按原始分辨率映射。
- 建议控制端以 `Ping/Pong` 检测链路存活，并在 `Bye` 后关闭连接。

## 7. 完整配对 Quickstart（curl）

以下示例假设：

```bash
BASE=https://rvnc.example.com
CTRL_ID=ctrl_phone_1
```

### 步骤 1：被控端注册

```bash
curl -sS "$BASE/api/v1/devices/register" \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "My Desktop",
    "platform": "windows",
    "fingerprint": "sha256:example",
    "version": "0.1.0"
  }' | tee /tmp/register.json

CODE=$(jq -r .device_code /tmp/register.json)
PASSWORD=$(jq -r .password /tmp/register.json)
AGENT_SECRET=$(jq -r .agent_secret /tmp/register.json)
PREAUTH=$(jq -r .headscale.preauth_key /tmp/register.json)
HOSTNAME=$(jq -r .headscale.hostname /tmp/register.json)
```

### 步骤 2：启动 sidecar 并上报心跳

```bash
TS_AUTHKEY="$PREAUTH" \
TS_HOSTNAME="$HOSTNAME" \
TS_CONTROLURL="https://headscale.example.com" \
./rvnc-tsnet &

IP4=$(curl -s http://127.0.0.1:1056/healthz | jq -r .ip4)

curl -sS "$BASE/api/v1/devices/heartbeat" \
  -H 'Content-Type: application/json' \
  -d "{
    \"device_code\": \"$CODE\",
    \"agent_secret\": \"$AGENT_SECRET\",
    \"tailscale_ip\": \"$IP4\",
    \"listen_port\": 21416,
    \"version\": \"0.1.0\"
  }"
```

### 步骤 3：控制端配对

```bash
curl -sS "$BASE/api/v1/pair" \
  -H 'Content-Type: application/json' \
  -d "{
    \"device_code\": \"$CODE\",
    \"password\": \"$PASSWORD\",
    \"controller_id\": \"$CTRL_ID\",
    \"controller_name\": \"My Phone\",
    \"platform\": \"android\"
  }" | tee /tmp/pair.json

TOKEN=$(jq -r .token /tmp/pair.json)
```

### 步骤 4：获取连接地址

```bash
curl -sS "$BASE/api/v1/connect" \
  -H 'Content-Type: application/json' \
  -d "{
    \"device_code\": \"$CODE\",
    \"token\": \"$TOKEN\",
    \"controller_id\": \"$CTRL_ID\"
  }"
```

响应中的 `tailscale_ip` 与 `port` 即控制流的连接目标。

### 步骤 5：被控端校验令牌（控制流 Hello 之后）

```bash
curl -sS "$BASE/api/v1/session/verify" \
  -H 'Content-Type: application/json' \
  -d "{
    \"device_code\": \"$CODE\",
    \"agent_secret\": \"$AGENT_SECRET\",
    \"token\": \"$TOKEN\",
    \"controller_id\": \"$CTRL_ID\"
  }"
```

### 步骤 6：管理员查看与吊销

```bash
ADMIN=your-admin-token

curl -sS "$BASE/api/admin/login" \
  -H 'Content-Type: application/json' \
  -d "{\"token\":\"$ADMIN\"}"

curl -sS "$BASE/api/admin/stats" \
  -H "Authorization: Bearer $ADMIN"

TOKEN_ID=$(jq -r .token_id /tmp/pair.json)
curl -sS -X DELETE "$BASE/api/admin/tokens/$TOKEN_ID" \
  -H "Authorization: Bearer $ADMIN"
```

### 步骤 7：被控端吊销某个控制端

```bash
curl -sS "$BASE/api/v1/agent/tokens/revoke" \
  -H 'Content-Type: application/json' \
  -d "{
    \"device_code\": \"$CODE\",
    \"agent_secret\": \"$AGENT_SECRET\",
    \"token_id\": \"$TOKEN_ID\"
  }"
```

## 8. 客户端实现要点

- 被控端心跳周期建议 30 秒；超过 90 秒判定离线。
- 控制端保存 `controller_id` 与 `token`；后续连接只调用 `/connect`，无需再次输入密码。
- 设备密码与 `agent_secret` 不得写入日志；令牌应使用系统安全存储。
- `tailscale_ip` 必须落在 `100.64.0.0/10`；控制端不应改连内网/回环地址。
- 所有业务流量走 tailnet；`rv-server` 只负责配对与控制面，不转发屏幕帧。
