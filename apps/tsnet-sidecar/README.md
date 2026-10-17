# rvnc-tsnet — RustVNC Tailscale 内嵌 sidecar

这个 Go 程序把 [tailscale.com/tsnet](https://pkg.go.dev/tailscale.com/tsnet) 嵌入桌面端，为 RustVNC 的 Rust 进程提供三项能力：

1. **入站转发**：在 tailnet 上监听 `:21416`（`RVNC_LISTEN_ADDR`），把每个控制端连接双向转发到本机 Rust 被控端的 `127.0.0.1:21416`（`RVNC_AGENT_ADDR`）。控制端因此只需连接被控端的 `100.x.y.z:21416`。
2. **SOCKS5 出站代理**：在本机回环地址 `127.0.0.1:1055`（`RVNC_SOCKS_ADDR`）提供无认证的 RFC 1928 CONNECT 代理，Rust 控制端通过它拨号 tailnet 地址（IPv4/IPv6/MagicDNS 域名）。这正是 `crates/rv-net/src/lib.rs` 中 `socks5://127.0.0.1:1055` 的约定。
3. **状态接口**：在本机回环地址 `127.0.0.1:1056`（`RVNC_STATUS_ADDR`）提供 `/healthz` 与 `/whoami`，供桌面应用读取本节点的 Tailscale IPv4 与在线状态。

sidecar 必须与桌面应用**同时运行**：Rust 进程启动后等待 sidecar 上线，再从 `/healthz` 读取 `ip4`，随后向配对后台发送心跳。

## 编译

需要 Go 1.23+。首次编译先拉取依赖并生成 `go.sum`（仓库中不包含 `go.sum`）：

```bash
cd apps/tsnet-sidecar
go mod tidy
go build -o rvnc-tsnet .          # Linux / macOS
go build -o rvnc-tsnet.exe .      # Windows（在 Windows 上执行）
```

升级 tailnet 依赖：

```bash
go get tailscale.com@latest
go mod tidy
```

本仓库固定 `tailscale.com v1.80.0`；如无特殊原因请保持客户端与 derper 版本接近。

## 运行

```bash
# Linux / macOS
export TS_AUTHKEY="..."             # Headscale 预授权密钥，见下文
export TS_HOSTNAME="rvnc-a2b3c4"     # 必须与配对后台分配的 hostname 一致
export TS_CONTROLURL="https://headscale.example.com"
./rvnc-tsnet
```

```powershell
# Windows PowerShell
$env:TS_AUTHKEY = "..."
$env:TS_HOSTNAME = "rvnc-a2b3c4"
$env:TS_CONTROLURL = "https://headscale.example.com"
.\rvnc-tsnet.exe
```

也可以使用 `scripts/run.ps1` / `scripts/run.sh`。脚本只设置无害的默认值，不会写入或打印任何密钥。

## 环境变量

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `TS_AUTHKEY` | **必填** | Headscale 预授权密钥。被控端从 `POST /api/v1/devices/register` 的 `headscale.preauth_key` 获取。不要写入日志或配置文件。 |
| `TS_HOSTNAME` | `rvnc-sidecar` | tailnet 节点名。**生产环境必须使用 `rvnc-<6 位设备码小写>`**，例如设备码 `A2B3C4` → `rvnc-a2b3c4`。配对后台按该名字反查节点 IP。 |
| `TS_STATE_DIR` | `./tsnet-state` | tsnet 状态目录，保存节点密钥与登录状态。首次加入后应持久化；再次启动仍建议提供预授权密钥。 |
| `TS_CONTROLURL` | 空（Tailscale 官方控制面） | 必须设置为自建 Headscale 的公开地址，例如 `https://headscale.example.com`。 |
| `TS_DERP_URL` | 空 | 仅为与配对后台的 `DERP_URL` 对齐而读取。**tsnet 不直接消费该变量**：DERP 区域由控制面下发，请把自建 derper 写进 Headscale 的 `derp.urls`。设置后程序会打印一条警告。 |
| `RVNC_AGENT_ADDR` | `127.0.0.1:21416` | 本机 Rust 被控端监听地址。 |
| `RVNC_LISTEN_ADDR` | `:21416` | tailnet 入站监听地址（格式必须为 `host:port`；为空 host 表示 tailnet 内所有接口）。 |
| `RVNC_SOCKS_ADDR` | `127.0.0.1:1055` | SOCKS5 监听地址。**保持回环**；该代理无认证。 |
| `RVNC_STATUS_ADDR` | `127.0.0.1:1056` | 状态 HTTP 监听地址。程序强制要求回环地址。 |
| `RVNC_LOG_LEVEL` | `info` | `debug` / `info` / `warn` / `error`。日志写 stderr。 |

## hostname 约定

配对后台在设备注册时生成 6 位设备码，并用 `rvnc-<code 小写>` 作为 Headscale 预授权密钥的 hostname：

```text
设备码 A2B3C4  ->  hostname rvnc-a2b3c4
```

被控端必须把注册响应中的 `headscale.hostname` 原样传给 `TS_HOSTNAME`。控制端/管理端通过 `GET /api/v1/node` 按该 hostname 解析 `100.x.y.z`，所以改名会导致 `device_offline`。

## 状态接口

```bash
curl -s http://127.0.0.1:1056/healthz
# {"ok":true,"hostname":"rvnc-a2b3c4","ip4":"100.64.0.7","online":true}

curl -s http://127.0.0.1:1056/whoami
# {"ok":true,"self":{...},"whois":{...}}
```

- `ok`：能否从 tsnet 取到状态。
- `online`：`BackendState == "Running"` 且本节点在线。
- `ip4`：本节点 `100.64.0.0/10` 地址，用于心跳上报。
- `/whoami` 仅监听回环，返回本节点 `Status()` 与 `WhoIs()` 信息；不要暴露到公网。

## SOCKS5 行为

- RFC 1928，仅支持 `CONNECT`，无认证（方法 `0x00`）。
- 支持 IPv4、IPv6 与域名（域名交给 tsnet 内的 MagicDNS 解析，不使用宿主机 DNS）。
- 成功后返回标准 reply，然后原样双向转发；半关闭（`CloseWrite`）会被传播。
- 失败映射：`0x01` 一般错误、`0x03` 网络不可达、`0x04` 主机不可达、`0x05` 拒绝连接、`0x06` 超时、`0x07` 命令不支持、`0x08` 地址类型不支持。
- 握手超时 15 秒，拨号超时 30 秒。

## 安全注意事项

- `TS_AUTHKEY` 是敏感凭据：只通过环境变量传入，不要写进脚本或提交到仓库。
- `RVNC_SOCKS_ADDR` 与 `RVNC_STATUS_ADDR` 保持 `127.0.0.1`；SOCKS5 无认证，暴露即等于把整个 tailnet 的拨号能力交给本机其他进程/网络。
- `RVNC_STATUS_ADDR` 若配置为非回环地址，程序会拒绝启动。
- Windows 上如启用防火墙，无需为 1055/1056 开放入站规则（仅回环）。
- 预授权密钥通常是一次性的；节点加入后状态保存在 `TS_STATE_DIR`，请限制该目录权限（程序创建时使用 `0700`）。

## 与配对流程的衔接

1. Rust 被控端调用 `POST /api/v1/devices/register`，得到 `device_code`、`password`、`agent_secret` 和 `headscale { control_url, preauth_key, hostname, derp_url }`。
2. 本 sidecar 以 `TS_AUTHKEY=headscale.preauth_key`、`TS_HOSTNAME=headscale.hostname` 启动，加入 Headscale tailnet。
3. 被控端读取 `http://127.0.0.1:1056/healthz` 的 `ip4`，调用 `POST /api/v1/devices/heartbeat` 上报 `tailscale_ip` 与 `listen_port=21416`。
4. 控制端配对后调用 `POST /api/v1/connect` 获得 `tailscale_ip` 与 `port`，再通过自己的 sidecar SOCKS5（或 Android 的 VpnService）连接该地址。
5. 控制端在 TCP 流上发送 RustVNC 控制协议 `Hello`，被控端调用 `/api/v1/session/verify` 校验免密令牌后开始视频/输入会话。

> PC 控制端同样需要加入 tailnet，但配对后台目前只在设备注册时签发预授权密钥。控制端可由管理员用 Headscale CLI 手动创建密钥，或由后续的“控制端注册”接口签发。
