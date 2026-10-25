# RustVNC Android 控制端

本目录是 RustVNC 的 Android 手机控制端脚手架：内嵌 Tailscale 网络层的接入点、配对
流程、设备列表、设置页，以及基于 RustDesk 思路的屏幕画面/输入控制协议客户端。

界面为 Jetpack Compose，深色渐变 + 毛玻璃卡片 + 卡片入场动画；设备码输入为 6 个
48×56 的独立输入格；所有主界面右上角都有设置齿轮。

> **先读这一条：** 本仓库没有、也不会自动下载 Tailscale 官方 Android SDK。构建中
> 网络层使用 `NoopTailscaleController`，它只会如实报告“未集成”。要访问 `100.x.y.z`
> 地址，必须按下面的“Tailscale 集成缺口”一节接入本地构建的 AAR。配对/设备管理/会话
> 界面与控制协议代码本身是完整的，可连接可直接路由的目标做联调。

---

## 1. 工具链

| 工具 | 版本/说明 |
| --- | --- |
| Android Studio | Ladybug (2024.2) 或更新 |
| JDK | 17 |
| Android SDK | Platform 35、Build Tools 35.0.0 |
| Gradle | 8.10.2（`gradle/wrapper/gradle-wrapper.properties` 已给出；**未提交 wrapper jar**） |
| Kotlin | 2.0.21 |
| AGP | 8.7.3 |
| Compose BOM | 2024.10.01 |
| Go | 1.22+（仅构建 Tailscale AAR 时需要） |
| gomobile | 最新版（仅构建 Tailscale AAR 时需要） |

主要依赖：Compose Material3、Navigation Compose、Lifecycle Compose、
`androidx.security:security-crypto`（EncryptedSharedPreferences）、OkHttp、
kotlinx.serialization、kotlinx.coroutines。

## 2. 构建

```bash
cd android

# 如果本机没有生成 wrapper jar：
gradle wrapper --gradle-version 8.10.2

./gradlew :app:assembleDebug
./gradlew :app:testDebugUnitTest   # 纯 JVM 的 bincode 编解码测试
```

在 Android Studio 中直接打开 `android/` 目录即可；首次同步前在
`android/local.properties` 写入：

```properties
sdk.dir=/path/to/Android/Sdk
```

`minSdk = 26`，`targetSdk = 35`，`compileSdk = 35`。

## 3. 目录结构

```text
android/
├── settings.gradle.kts
├── build.gradle.kts
├── gradle.properties
├── gradle/wrapper/gradle-wrapper.properties
├── app/
│   ├── build.gradle.kts
│   ├── proguard-rules.pro
│   └── src/
│       ├── main/
│       │   ├── AndroidManifest.xml
│       │   ├── res/…                       图标、主题、网络安全配置
│       │   └── java/com/rustvnc/client/
│       │       ├── MainActivity.kt
│       │       ├── RustVncApp.kt
│       │       ├── AppContainer.kt
│       │       ├── data/                    DTO、配对仓库、加密存储、设置
│       │       ├── net/                     Tailscale 包装、TCP 会话、前台服务、VpnService 占位
│       │       ├── wire/                    bincode 2 standard 编解码、帧格式、消息定义
│       │       └── ui/                      Compose 主题、组件、配对/设备/设置/会话页面
│       └── test/java/com/rustvnc/client/wire/WireCodecTest.kt
└── tailscale-sdk/                           网络层接口与 Noop 实现（本地库模块）
```

## 4. 配对后台 API（已实现）

所有请求都是 `POST`，`Content-Type: application/json`。`{server}` 是设置页填写的
服务器地址（去掉末尾 `/`）。字段名与 `crates/rv-proto/src/api.rs` 完全一致（snake_case）。
错误响应统一为：

```json
{ "error": { "code": "unauthorized", "message": "设备码或密码不正确" } }
```

### 4.1 配对

```http
POST {server}/api/v1/pair
```

```json
{
  "device_code": "ABC234",
  "password": "由被控端显示的密码",
  "controller_id": "android-<uuid>",
  "controller_name": "Android 控制端",
  "platform": "android"
}
```

成功响应：

```json
{
  "token": "rvt_…",
  "token_id": "tok_…",
  "device_code": "ABC234",
  "device_name": "工作站",
  "tailscale_ip": "100.64.0.2",
  "online": true,
  "server_version": "0.1.0"
}
```

Android 端在此之后立即把 `token` 写入 EncryptedSharedPreferences，并保存设备记录。

### 4.2 解析连接地址

```http
POST {server}/api/v1/connect
```

```json
{
  "device_code": "ABC234",
  "token": "rvt_…",
  "controller_id": "android-<uuid>"
}
```

成功响应：

```json
{
  "device_code": "ABC234",
  "device_name": "工作站",
  "tailscale_ip": "100.64.0.2",
  "port": 21416,
  "online": true
}
```

常见错误：`401`（令牌无效/已吊销/过期，或控制器 ID 不匹配）、`409
device_offline`（被控端未加入虚拟网络或离线）、`429`（尝试过于频繁，会带
`Retry-After`）。设备列表的刷新与进入会话时都会调用该接口。

控制端 **不调用** `/api/v1/devices/register`：只有被控端需要注册并获得设备码。

### 4.3 本地移除

设备列表的“移除设备”只删除本机记录与令牌。服务器端令牌仍然有效，需由被控端或
管理后台吊销（公开 API 中没有控制端可直接调用的吊销接口）。

## 5. 控制协议（已实现）

### 5.1 传输

- 在 Tailscale/WireGuard 隧道内建立单条 TCP 连接：`tailscale_ip:port`（端口来自
  `/api/v1/connect`）。
- 每帧：`u32 小端长度 || bincode 2 body`，长度上限 16 MiB（0 长度直接拒绝），与
  `crates/rv-engine/src/codec.rs` 一致。
- 连接后客户端立即发送 `Hello`，等待 `ServerMessage::Hello`；连接超时 12 s，
  握手后读超时 30 s。客户端每 5 s 发送一次 `Ping { id }`。
- 视频帧为 JPEG；Android 端用 `BitmapFactory` 解码，像素超过约 200 万时按 2 的幂
  下采样。过期帧通过 conflated channel 丢弃，避免延迟累积。

### 5.2 bincode 2 `standard()` 规则

本项目锁定 bincode **2.0.1**（见 `Cargo.lock`）。它的 variable-int 编码不是 LEB128，
而是带类型标记的方案（`src/varint/encode_unsigned.rs`）：

| 数值范围 | 编码 |
| --- | --- |
| `0..=250` | 单字节，值为自身 |
| `251..=65535` | `0xFB` + u16 小端 |
| `65536..=4294967295` | `0xFC` + u32 小端 |
| 更大（u64/usize） | `0xFD` + u64 小端 |

- 枚举变体序号使用 **u32** 形式；字符串/Vec 长度使用 **u64/usize** 形式，随后跟
  原始字节。字符串长度是 **UTF-8 字节数**，不是字符数。
- `[u8; 4]`：固定 4 字节，无长度前缀。
- `bool`：1 字节；`f64`：8 字节小端 IEEE-754。
- 解码器按目标整数宽度校验标记：u16 不接受 u32 标记，u32 不接受 u64 标记，u64 不
  接受 u128 标记，与 bincode 的类型化解码器一致。

### 5.3 ClientMessage

| 变体 | 编号 | 字段 |
| --- | --- | --- |
| Hello | 0 | magic `[u8;4]`（`"RVNC"`）、wire_version `u16`、device_code、token、controller_id、controller_name、platform |
| Input | 1 | InputEvent（见下） |
| Ping | 2 | id `u64` |
| Bye | 3 | 无 |

InputEvent：

| 变体 | 编号 | 字段 |
| --- | --- | --- |
| MouseMove | 0 | x `f64`、y `f64`（归一化 0..1） |
| MouseButton | 1 | button 枚举、pressed `bool` |
| Wheel | 2 | delta_x `f64`、delta_y `f64`（单位是“格”；被控端乘以 120） |
| Key | 3 | code `String`（DOM `KeyboardEvent.code`，如 `KeyA`）、pressed `bool` |
| Text | 4 | text `String`（IME/Unicode） |
| Clipboard | 5 | text `String` |

MouseButton 的变体顺序严格取自 `rv_proto` 的声明顺序：
`Left=0, Right=1, Middle=2, Back=3, Forward=4`。

### 5.4 ServerMessage

| 变体 | 编号 | 字段 |
| --- | --- | --- |
| Hello | 0 | wire_version `u16`、accepted `bool`、session_id、device_name、width `u32`、height `u32`、fps `u16` |
| Video | 1 | seq `u64`、width `u32`、height `u32`、timestamp_ms `u64`、format（Jpeg=0）、data `Vec<u8>` |
| Pong | 2 | id `u64` |
| Error | 3 | code、message |

### 5.5 触摸映射

- 画面使用 `ContentScale.Fit`，严格保持宽高比，黑边不拉伸。
- 手指坐标先换算出画面在屏幕上的 letterbox 内容矩形，再归一化到 0..1；点在黑边上
  会被忽略。
- 单击 = 左键；长按 = 右键；双指拖动 = 滚轮（移动阈值 20 px，除以 80 得到格数）。
- 软键盘/输入法通过一个隐藏 `BasicTextField` 把新增文本作为 `Text` 事件发送；退格
  发送 `Key("Backspace")`，完成键发送 `Key("Enter")`。

## 6. Tailscale 集成缺口（必读）

### 6.1 现状

- `tailscale-sdk` 模块只提供 `TailscaleController` 接口和
  `NoopTailscaleController`。
- `AppContainer` 明确注入 `NoopTailscaleController`；设置页会显示“未集成”，尝试
  启动会返回中文错误。
- `RustVncVpnService` 已在 Manifest 中声明（`BIND_VPN_SERVICE`），但**不会建立
  隧道**，只是集成点。
- 因此，当前 APK 无法访问 `100.x.y.z` 地址；只有目标地址能被设备直接路由时，
  会话才能连通。

### 6.2 为什么不能自动获取

Tailscale 官方的 Android 客户端不是一个发布在 Maven 上的 AAR。`gomobile` 生成的
`libtailscale.aar` 只是 Go 核心的 JNI 绑定，它本身不会：

- 申请 Android 的 `VpnService` 权限；
- 建立 TUN 接口并把文件描述符交给 Go 网络栈；
- 完成登录 / pre-auth key 流程；
- 处理 Android 的后台保活。

这些需要复用或参考 `tailscale-android` 的 `IPNService`/`VpnService` 代码，或者自己
实现一个控制器来完成。

### 6.3 建议的接入步骤

1. 用 Go 1.22+ 构建本地 AAR（路径以实际 tailscale 仓库为准）：

   ```bash
   go install golang.org/x/mobile/cmd/gomobile@latest
   gomobile init

   cd <tailscale-repo>/cmd/libtailscale
   gomobile bind -target=android -androidapi 26 -o libtailscale.aar .
   ```

2. 把 `libtailscale.aar` 放到 `android/app/libs/`，在
   `android/app/build.gradle.kts` 中取消注释：

   ```kotlin
   implementation(files("libs/libtailscale.aar"))
   ```

3. 在 `tailscale-sdk`（或 app 的 net 包）里实现 `TailscaleController`：
   - 在 Activity 中先调用 `VpnService.prepare(context)` 获得用户授权；
   - 由 VpnService 建立 TUN 并把 fd/路由配置交给 AAR；
   - `start(preAuthKey, controlUrl, hostname)` 完成后把 `state` 更新为
     `Running`，并让 `tailnetIp()` 返回 `100.x.y.z`；
   - `stop()` 关闭 TUN 并清理状态。
4. 在 `AppContainer` 中把 `NoopTailscaleController()` 替换为新实现。

### 6.4 控制端的 pre-auth key

配对服务器提供控制端入网接口 `POST /api/v1/controllers/enroll`：设置页
“网络（Tailscale）”中的“自动获取网络接入密钥”按钮会调用它，并把返回的
`preauth_key` / `control_url` 自动填入表单（密钥同样存入
EncryptedSharedPreferences）。该接口需要服务器已配置 Headscale API Key，
并受来源 IP 的注册限流保护。

也可以手动创建（例如服务器未配置自动签发时）：

```bash
headscale preauthkeys create --user <user> --reusable=false
```

把控制面地址和该密钥填入设置页“网络（Tailscale）”一栏。没有集成真实 AAR 时，
该面板的“应用网络配置”按钮保持禁用，避免造成“已经联网”的错觉。

如果所选 AAR 暴露交互式登录（而不是 pre-auth key），可以在 `TailscaleController`
的实现里把空密钥解释为“走登录流程”，并相应放宽设置页的校验；当前脚手架按
pre-auth key 路径实现。

### 6.5 内置客户端配置（env-back 模板）

仓库根目录的 `env-back` 是**客户端配置示例模板**，构建脚本不会自动读取它。
需要把 Headscale/DERP/密钥内置到 APK 时：

1. 复制模板并填写真实值：

   ```bash
   cp env-back .env
   # 编辑 .env
   ```

2. **重新构建 APK**：

   ```bash
   gradle -p android :app:assembleRelease :app:bundleRelease
   ```

3. 构建时 `.env` 会写入 APK 的 `assets/embedded_env.txt`，并显示在
   **设置 → 内置配置（.env）**。

注意：

- 修改 `.env` 后必须重新编译；只重启应用不会更新已内置的内容。
- `.env` 是本机真实配置，已被 `.gitignore` 忽略，不要提交；`env-back` 是仓库中的
  占位符模板。
- CI 构建没有 `.env`，因此不会内置密钥。
- 配置会编译进 APK，任何人都能提取；不要公开分发包含生产密钥的构建。

## 7. 安全说明

- 配对令牌与控制器 pre-auth key 保存在 **EncryptedSharedPreferences**
  （MasterKey + AES256-SIV 键加密 + AES256-GCM 值加密）。
- `allowBackup="false"`，避免令牌随系统备份泄露。
- 没有安装 OkHttp 日志拦截器；代码中不打印 token、密码或请求体。
- 默认禁止明文流量；`network_security_config.xml` 仅为 `localhost`、`127.0.0.1`
  和模拟器宿主 `10.0.2.2` 放行 HTTP，其余地址必须 HTTPS。没有实现证书固定。
- 本地“移除设备”不等于服务器吊销；如需吊销请在管理后台操作，或由被控端移除。
- 控制协议本身没有额外的应用层加密：依赖 Tailscale/WireGuard 隧道加密。

## 8. 已知限制与假设

- Tailscale 网络层未集成（见第 6 节）。
- 画质/FPS 仅是本地偏好，v1 协议不协商；被控端使用自己的采集配置。
- 后台会话依赖前台服务维持进程，但控制引擎当前位于 SessionViewModel：Activity
  被结束时 ViewModel 清理并断开。完全后台常驻需要把引擎迁入服务。
- 剪贴板事件已在编解码层实现，但 UI 尚未提供剪贴板发送入口。
- 未实现硬件键盘到 DOM key code 的完整映射（IME 文本通过 Text 事件发送）。
- 控制端没有 Revoke Token 的公开 API；本地移除不吊销服务器令牌。
- 单元测试只覆盖纯 JVM 的 wire 编解码；UI/Tailscale 集成需真机验证。
- 采用内联中文字符串（`strings.xml` 仅保存应用名与通知文案），保持 UI 文案集中
  在 Composable 中。

## 9. 联调建议

1. 先配置 `https://` 配对服务器地址，完成一次配对，确认设备列表出现设备。
2. 让被控端与手机处于可互相路由的网络（或完成 AAR 集成），确认 `/api/v1/connect`
   返回 `online: true`。
3. 进入会话，检查握手（`Hello` 的 magic/version）、首帧与 Ping/Pong。
4. 若出现“协议版本不兼容”，确认两端都是 `WIRE_VERSION = 1`。
5. 若出现“无法路由到被控端”，查看设置页网络层状态；Noop 构建下这是预期行为。

## 10. 变更记录

### [2026-10-18] Android 控制端脚手架

- 新增 Gradle/Compose 工程、Manifest、网络安全配置与 Launcher 图标。
- 新增配对（6 格输入、密码显示切换、加载态）、设备列表（在线点、刷新、长按/溢出
  移除）、设置（服务器、控制端名称、Tailscale 预授权、画质/FPS、关于）页面。
- 新增 RustVNC bincode 2 `standard()` 编解码、16 MiB 帧格式、TCP 会话、JPEG 解码
  与输入事件发送。
- 新增会话页：等比画面、letterbox 坐标映射、单击/长按/双指滚轮、IME 文本、悬浮
  工具条、全屏、断开确认。
- 新增 TailscaleController 接口与 Noop 实现，明确记录集成缺口；未伪造网络能力。
- 新增纯 JVM 编解码单元测试（`WireCodecTest`）。
- 说明：按本次任务约束只修改 `android/` 目录，因此项目根目录的 `CHANGELOG.md`
  未改动；本节的变更记录暂存在此文件。
