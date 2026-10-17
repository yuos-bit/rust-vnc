# NOTICE

RustVNC 以 **GNU Affero General Public License v3.0**（AGPL-3.0）发布，完整文本见 `LICENSE`。

## 第三方组件

| 组件 | 许可 | 使用方式 |
| --- | --- | --- |
| Tailscale / tsnet | BSD-3-Clause | `apps/tsnet-sidecar` 通过 Go 模块引用；Android 端需引入官方 Tailscale 网络栈产物 |
| Headscale | BSD-3-Clause | 服务端部署，不修改其源码 |
| RustDesk | AGPL-3.0 | 本快照**未**包含 RustDesk 源码；`rv-capture` / `rv-input` / `rv-engine` 为独立实现，后续替换/移植 RustDesk 模块时继续遵循 AGPL-3.0 |
| Jetpack Compose / AndroidX | Apache-2.0 | Android 客户端 |
| Tauri / Wry / Tao | Apache-2.0 / MIT | 桌面端 |

## 合规说明

- 本项目为 AGPL-3.0 作品。若通过网络向用户提供修改后的版本，必须按 AGPL-3.0 第 13 条向用户提供对应源代码。
- 若后续集成 RustDesk 代码，RustDesk 的 AGPL-3.0 条款同样适用；请在发行包中保留其版权声明与许可文本。
- Tailscale 商标与品牌归 Tailscale Inc. 所有；本项目仅使用其开源网络栈，不声称与 Tailscale Inc. 存在隶属关系。
- Android 端构建 Tailscale AAR 时，请遵守其 BSD-3-Clause 许可及 VpnService 使用规范。
