# 变更记录

本文件遵循 `claude.md` §8 的记录规范。每次修改追加条目；整体功能完成后记录代码审计与漏洞扫描结果。

## [2026-10-18] 初始实现：端到端远程控制 MVP

### 变更内容

- **工作区**：新建 Cargo workspace（edition 2024，Rust 1.99），成员包括 `rv-core`、`rv-proto`、`rv-net`、`rv-capture`、`rv-input`、`rv-engine`、`rv-server`、`rv-pc`；补充 `.gitignore`、`.dockerignore`、`LICENSE`（AGPL-3.0）、`NOTICE.md`、`README.md`。
- **rv-core**：6 位无歧义设备码生成/归一化、Argon2id 密码哈希、随机令牌与 SHA-256 摘要、恒定时间比较、滑动窗口失败限流、等比适配几何。新增 17 项单元测试。
- **rv-proto**：配对/管理 API DTO、控制线协议（`u32_le` 长度前缀 + bincode 2 `standard()`）、Hello/Video/Input/Ping/Pong/Error 消息与 16 MiB 帧上限；新增编码往返测试。
- **rv-net**：直连与 loopback SOCKS5（tsnet sidecar）传输、Tailscale CGNAT `100.64.0.0/10` 校验；新增 3 项测试。
- **rv-capture**：Windows GDI 虚拟桌面采集（多显示器）、BGRA→RGB、按比例可选降采样、JPEG 编码；新增 3 项测试。
- **rv-input**：Windows `SendInput` 鼠标/滚轮/侧键/键盘/Unicode 注入，DOM `code` → Windows VK 映射；新增 1 项测试。
- **rv-engine**：配对 API 客户端、被控端引擎（注册、心跳、令牌二次校验、会话并发限制、输入注入、帧推流、空闲超时）、控制端引擎（解析设备、握手、帧接收、输入/Ping、断线处理）；新增编解码测试及一个可手动运行的端到端截屏会话测试。
- **rv-server**：Axum 配对后台、SQLite（设备、令牌、配对审计、设置）、Headscale REST 客户端（Pre-Auth Key、节点地址解析）、按设备码/IP 的双层限流、Argon2 时序均衡、管理员 Bearer 鉴权、内嵌 Web 管理台（设备总览、令牌、配对记录、设置、Headscale 节点测试、右上角设置齿轮、毛玻璃/卡片动画）。
- **PC 端（Tauri 2）**：控制端/被控端双角色；6 格设备码输入、免密设备列表、设备码大字号展示、状态呼吸点、密码显示/修改、已配对控制端解除、设置页；会话视图采用 `object-fit: contain` 等比显示，支持鼠标/键盘/滚轮/Unicode 文本、全屏、断开、自动隐藏工具条；设置与身份使用 Windows DPAPI 加密。
- **Android 端**：Jetpack Compose 完整源码脚手架（配对、设备列表、设置、会话、前台服务、VpnService 声明），实现 bincode 2 线协议编解码与等比触摸映射；Tailscale AAR 集成点以 `TailscaleController` 接口和 `NoopTailscaleController` 明确标注为待接入。
- **部署**：Headscale 0.26.1 + 自建 derper + rv-server + Caddy 的 docker-compose、Headscale 配置模板、Caddyfile、`server.Dockerfile`；Go `tsnet` sidecar（tailnet 入站转发 + SOCKS5 + loopback 状态接口）。
- **文档/脚本**：`docs/API.md`、`docs/DEPLOYMENT.md`、`docs/ARCHITECTURE.md`、Android/部署/边车 README、`scripts/gen_icons.py`、`scripts/smoke-test.sh`、UI 截图。

### 影响范围

- 涉及模块：全部（配对后台、Web 管理台、桌面被控端、桌面控制端、Android 客户端、部署资产）。
- 新增依赖：axum、rusqlite（bundled）、reqwest（rustls）、tauri 2、argon2、bincode 2、image、tokio 等。

### 整体完成审计

- [x] 代码审计：`cargo fmt --all --check`、`cargo clippy --workspace --all-targets`
- [x] 漏洞扫描：`cargo-audit 0.22.2 audit --file Cargo.lock`
- [x] 测试：`cargo test --workspace` + 配对后台冒烟脚本 + 桌面 UI 端到端
- 结果详见下一条目。

---

## [2026-10-18] 验证、缺陷修复与最终审计

### 变更内容

- **修复设备码生成器死循环**（`rv-core/src/code.rs`）：拒绝采样边界原为 `(256 / 32 * 32) as u8`，在 `u8` 溢出为 0，导致随机字符永不写入。改为 `224`，并补充 2000 次唯一性测试。
- **修复 UI `hidden` 属性失效**：Tauri 前端 CSS 的 `display` 覆盖了 `hidden`，导致隐藏的“设置”标签页和部分面板仍显示；新增全局 `[hidden] { display: none !important; }`。
- **持久化控制端 ID**：首次启动生成的 `controller_id` 原先只存在内存中，重启后会变化并使已保存令牌失效；现在启动时立即写回 `settings.json`。
- **修复被控端首帧状态竞态**：心跳首次触发早于 Tauri 命令返回，导致“在线”事件被“启动中”快照覆盖；心跳循环增加 450 ms 初始延迟。
- **Clippy 零告警清理**：合并嵌套 `if let`（let-chains）、派生 `Default`、`as_chunks`、补充 `too_many_arguments` 允许项。
- **新增控制端入网接口**：`POST /api/v1/controllers/enroll` 为独立 PC/Android 控制端签发一次性 Pre-Auth Key（受 IP 限流，不落库）；PC 设置页新增“获取网络接入密钥”，Android 设置页新增“自动获取网络接入密钥”。
- **修正 Headscale Pre-Auth Key 请求体**：按 Headscale 0.26 Proto 移除不存在的 `hostname` 字段，加入 1 小时 RFC3339 过期时间，用户 ID 以 uint64 发送。
- **图床/图标**：使用 Python 标准库生成 16–256 px 的多尺寸 ICO 与 PNG 图标。
- **Docker 构建**：`server.Dockerfile` 改用 `rust:1.99-bookworm`，补齐根 `.dockerignore`。
- **文档**：更新 README 的实现状态、界面预览与验证结果；新增 `docs/ARCHITECTURE.md`。

### 影响范围

- 涉及模块：`rv-core`、`rv-engine`、`rv-server`、`rv-pc`（前端与启动逻辑）、Android 设置页、部署文件、文档。
- 不涉及协议版本变更（`WIRE_VERSION` 仍为 1）。

### 整体完成审计

- [x] **代码审计**
  - `cargo fmt --all --check`：通过。
  - `cargo clippy --workspace --all-targets`：**0 warning / 0 error**。
  - 人工检查：SQL 全部参数化；管理台与桌面 UI 对用户可控文本做 HTML 转义；日志不输出密码、令牌或 agent secret；未知设备执行等价 Argon2 校验以降低时序枚举风险。
- [x] **漏洞扫描**
  - `cargo-audit 0.22.2`：**0 个漏洞**。
  - 3 条警告：`bincode 2.0.1` 已停止维护（RUSTSEC-2025-0141）、`proc-macro-error 1.0.4` 已停止维护（RUSTSEC-2024-0370，Tauri 构建期传递依赖）、`glib 0.18.5` soundness 公告（RUSTSEC-2024-0429，仅 Linux GTK 目标；Windows 构建不启用）。
  - 后续建议：将线协议迁移到活跃维护的格式（如 postcard）或为 bincode 固定版本并提供迁移计划；Linux 部署时升级 Tauri/GTK 依赖链。
  - Android Gradle 依赖与 Go 模块因本机无 JDK/Android SDK/Go 未执行扫描；部署前应在对应工具链上运行 `./gradlew dependencyCheckAnalyze`（或等价 SCA）与 `govulncheck ./...`。
- [x] **测试结果**
  - `cargo test --workspace`：**36 passed / 0 failed / 1 ignored**。忽略项为需要交互式桌面的真实截屏测试，单独执行 `cargo test -p rv-engine --test session_roundtrip -- --ignored --nocapture` 通过（约 2 秒，收到 JPEG 帧）。
  - `scripts/smoke-test.sh`：**15/15 通过**（注册、心跳、错误密码 401、配对、连接、会话校验、控制端入网在未配置 Headscale 时返回 400、管理 API 鉴权、统计、吊销、吊销后校验/连接拒绝）。
  - **桌面 UI 端到端**：启动被控服务 → WebView 显示设备码与密码 → 同机控制端输入设备码/密码配对 → 免密连接 → 1920×1080 画面按原比例显示、工具条与状态正确。
  - Web 管理台静态资源：`/` 308 跳转、`/admin/`、`styles.css`、`app.js` 均 200 且 MIME 正确。
- **未验证项（环境限制，已在文档中标注）**
  - Android 源码未编译（无 JDK/Android SDK）；Tailscale AAR 未接入。
  - Go tsnet sidecar 未编译（无 Go），`go.sum` 需在有网络与 Go 的机器上生成。
  - Headscale/DERP 容器编排未实际启动（无 Docker）。
  - 真实 Headscale 的 Pre-Auth Key 与节点地址解析未联调。

---

## [2026-10-18] 桌面端新增暗黑 / 明亮主题切换

### 变更内容

- 顶部导航右上角新增主题切换按钮：暗黑主题显示太阳图标，明亮主题显示月亮图标，点击即时切换。
- 新增明亮主题设计令牌：浅色渐变背景（`#e0c3fc → #8ec5fc → #f5d5e0`）、白色半透明玻璃卡片、深色文字、蓝色/紫色主按钮、适配输入框/标签/提示/弹窗/Toast/滚动条等全部控件。
- 新增 Tauri 命令 `set_theme`：校验 `dark` / `light`，写入 `settings.json`，并通过 `settings://updated` 同步前端；应用启动时恢复上次主题。
- 远程会话画面区域保持黑底与等比显示，不受主题影响。
- 新增截图：`docs/screenshots/pc-theme-dark.png`、`pc-theme-light.png`、`pc-theme-light-agent.png`。

### 影响范围

- `apps/pc/ui/index.html`、`styles.css`、`app.js`
- `apps/pc/src/commands.rs`、`apps/pc/src/lib.rs`
- `apps/pc/src/config.rs` 的 `theme` 字段（已有字段，已接入持久化）
- `README.md` 界面预览与功能列表

### 整体完成审计

- [x] 代码审计：`cargo fmt --all --check` 通过；`cargo clippy -p rv-pc --all-targets` 0 warning / 0 error；`node --check apps/pc/ui/app.js` 通过。
- [x] 漏洞扫描：`cargo-audit 0.22.2` 重新扫描，0 个漏洞，3 条既有警告（bincode / proc-macro-error 停止维护、Linux-only glib 公告）。
- [x] 功能验证：实际启动桌面端，点击太阳图标切换到明亮主题，界面即时更新并显示“已切换为明亮主题”；`settings.json` 中 `theme` 写为 `light`，重启后可恢复。
- 结果：暗黑 / 明亮两套主题的控制端与被控端截图均已生成；会话区域保持黑底等比显示。
