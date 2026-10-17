# tailscale-sdk（占位模块）

这个 Android Library 模块只定义 RustVNC 控制端需要的网络层契约，**不包含**
Tailscale 的官方实现，也不会自动下载任何 AAR。

## 为什么存在

官方 Tailscale Android SDK 没有发布到 Maven。应用的其他部分通过
`TailscaleController` 与网络层解耦：

```kotlin
interface TailscaleController {
    val state: StateFlow<TailscaleState>
    suspend fun start(preAuthKey: String, controlUrl: String, hostname: String): Result<Unit>
    fun stop()
    fun tailnetIp(): String?
    fun isRunning(): Boolean
}
```

当前实现是 `NoopTailscaleController`：`state` 恒为 `Unavailable`，`start` 返回
带说明的失败结果。这是刻意的——界面必须如实显示“未集成”，而不是假装隧道已建立。

## 如何替换为真实实现

1. 构建本地 `libtailscale.aar`（见 `android/README.md` 第 6 节）。
2. 把 AAR 放进 `app/libs/`，在 `app/build.gradle.kts` 中启用
   `implementation(files("libs/libtailscale.aar"))`。
3. 新建例如 `AarTailscaleController` 实现本接口：
   - 在 Activity 中先 `VpnService.prepare()` 获得授权；
   - 由 `VpnService.Builder` 建立 TUN，并把 fd / 路由交给 AAR 的 Go 网络栈；
   - 使用 Headscale 控制面地址与预授权密钥加入 tailnet；
   - `start` 成功后把 `state` 置为 `TailscaleStatus.Running`，`tailnetIp()` 返回
     分配到的 `100.x.y.z`；
   - `stop()` 关闭 TUN 并更新状态。
4. 在 `com.rustvnc.client.AppContainer` 中把
   `NoopTailscaleController()` 替换为你的实现。

`app/src/main/java/com/rustvnc/client/net/RustVncVpnService.kt` 是 Manifest 中已声明的
VpnService 集成点；真实实现可以复用它，也可以自行声明等价的服务（需要
`android:permission="android.permission.BIND_VPN_SERVICE"`）。

## 注意

- 永远不要把预授权密钥、令牌或 WireGuard 私钥写入日志。
- `state.message` 会直接显示在设置页，必须是用户可读且不含秘密的中文文本。
- `libtailscale.aar` 只用 gomobile 绑定 Go 核心，并不会自动处理 Android VPN
  权限与 TUN 生命周期；这部分需要按 `tailscale-android` 的做法实现。
