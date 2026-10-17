# 自动构建与发布（GitHub Actions）

RustVNC 使用两个工作流：

| 工作流 | 文件 | 触发 | 作用 |
| --- | --- | --- | --- |
| **Release** | `.github/workflows/release.yml` | push 到 `main`、手动运行 | 判断是否发布；构建 Android APK/AAB + Windows x64；创建 tag 与 GitHub Release |
| **Generate Android signing keystore** | `.github/workflows/generate-android-keystore.yml` | 手动运行 | 生成/校验自签名 Android 签名证书并放入 Actions 私有缓存 |

---

## 1. 触发规则

版本判断由 `.github/scripts/next-version.sh` 完成：

1. **没有任何 `vMAJOR.MINOR.PATCH` tag**：立即发布，首个版本为 **`v0.1.0`**。
2. **已有最新 tag**：计算 `git rev-list --count <tag>..HEAD`。
   - 新增提交数 **≥ 10**：发布，补丁号 +1（`v0.1.0` → `v0.1.1` → `v0.1.2`…）。
   - 少于 10：跳过构建与发布（工作流仍然成功结束）。
3. **手动运行 Release**：可勾选 `force` 忽略 10 次间隔立即发布；版本号仍自动 +1。

> 工作流只在 `main` 分支的 push 上触发，tag 由发布动作创建。
> tag 只会在 Android 与 PC **都构建成功**后创建；构建失败不会消耗一次发布机会，下一次 push 仍会重试。

示例时间线：

```text
第 1 次 push（启用工作流后）        -> v0.1.0
之后第 1~9 次 push                  -> 跳过
距 v0.1.0 第 10 次 push             -> v0.1.1
再之后第 10 次 push                  -> v0.1.2
```

版本号的语义：`MAJOR.MINOR.PATCH`，自动发布只递增 `PATCH`。需要 `0.2.0` / `1.0.0` 时，手动创建对应的 tag 后，下次发布会从该 tag 继续 +1。

---

## 2. 版本号注入

每次发布构建都会把计算出的版本写入所有端，不修改仓库源文件：

| 端 | 注入方式 |
| --- | --- |
| Android | Gradle `-PrvncVersionName` / `-PrvncVersionCode`，写入 APK/AAB 的 `versionName`/`versionCode` |
| PC（Rust workspace） | CI 中修改 `[workspace.package] version`，所有 crate（含 `rv-server`）使用该版本 |
| PC（Tauri） | CI 中修改 `apps/pc/tauri.conf.json` 的 `version`，用于 Windows 文件版本与 About 页 |
| Git | 创建 tag `vX.Y.Z` 与同名 GitHub Release |

`versionCode` 计算：`major * 1000000 + minor * 1000 + patch`（`0.1.0` → `1000`，`0.1.1` → `1001`）。

---

## 3. 构建产物

Release 页面会附带：

- `RustVNC-<version>-android.apk` — 已签名的 release APK，可直接安装（需允许未知来源）
- `RustVNC-<version>-android.aab` — 已签名的 Android App Bundle
- `RustVNC-<version>-windows-x64.zip` — 解压后运行 `rustvnc.exe`（需要 WebView2；Windows 10/11 自带）
- `android-signing-fingerprint.txt` — 本次 APK 签名证书的 SHA-256 指纹
- `SHA256SUMS.txt` — 所有产物的 SHA-256

> PC 端当前发布的是免安装 exe，不包含 Tauri 的 NSIS/MSI 安装包。若需要安装包，可在本地执行 `npx @tauri-apps/cli@2 build`。

---

## 4. Android 自签名证书

### 4.1 默认行为（无需配置）

`Release` 工作流在构建前调用 `.github/scripts/ensure-android-keystore.sh`：

1. 从 Actions 缓存 `rustvnc-android-keystore-v1` 恢复 `android/keystore/release.p12`；
2. 缓存不存在时，用 `keytool` 生成 RSA-4096、有效期 10000 天的**自签名 PKCS#12** 证书；
3. 写入 `android/keystore.properties`（已被 `.gitignore` 忽略）；
4. 构建时自动签名，并把证书 SHA-256 指纹写入 release 说明。

证书**不会**提交到仓库、不会作为 artifact 上传、也不会打印到日志。它只存在于：

- GitHub Actions 的私有缓存；
- 当次构建的临时 runner 文件系统（构建结束后销毁）。

也可以手动运行 **Generate Android signing keystore** 工作流来预先生成证书，并在 Step Summary 查看指纹。

### 4.2 生产环境建议：使用固定密钥

Actions 缓存可能在长期无构建后被 GitHub 回收；若缓存丢失会生成新证书，导致新 APK 无法覆盖安装旧版本。生产发布建议把密钥固定到仓库 Secrets：

1. 在本机生成 PKCS#12（需要 JDK）：

   ```bash
   keytool -genkeypair -v \
     -keystore release.p12 -storetype PKCS12 \
     -alias rustvnc -keyalg RSA -keysize 4096 -validity 10000 \
     -storepass '你的强密码' -keypass '你的强密码' \
     -dname "CN=RustVNC, OU=Self-Signed, O=RustVNC, L=Local, ST=Local, C=CN"

   # Linux/macOS
   base64 -w0 release.p12 > release.p12.b64
   # Windows PowerShell
   [Convert]::ToBase64String([IO.File]::ReadAllBytes("release.p12")) | Set-Content release.p12.b64
   ```

2. 打开仓库 **Settings → Secrets and variables → Actions → New repository secret**，添加：

   | Secret | 必填 | 说明 |
   | --- | --- | --- |
   | `ANDROID_KEYSTORE_BASE64` | 是 | `release.p12.b64` 的完整内容（单行） |
   | `ANDROID_KEYSTORE_PASSWORD` | 是 | keystore 密码 |
   | `ANDROID_KEY_ALIAS` | 否 | 默认 `rustvnc` |
   | `ANDROID_KEY_PASSWORD` | 否 | 默认与 `ANDROID_KEYSTORE_PASSWORD` 相同；PKCS#12 要求两者一致 |
   | `ANDROID_KEYSTORE_TYPE` | 否 | 默认 `PKCS12`；若提供 JKS 密钥库请设为 `JKS` |

3. 重新运行 Release（或等待下一次自动发布）。工作流会优先使用 Secret 中的 keystore。

> 不要把 `.p12`、base64 文件或密码提交到仓库；`.gitignore` 已覆盖常见密钥扩展名。

### 4.3 轮换证书

1. 生成新证书并按 4.2 更新 Secrets；
2. 手动运行 Release（勾选 `force`）发布一个使用新签名的版本；
3. 用户需要卸载旧版后安装新版（Android 不允许同包名不同签名覆盖）。

---

## 5. 手动操作

### 5.1 立即发布

Actions → **Release** → Run workflow → 勾选 `force` → Run。

### 5.2 生成/检查签名证书

Actions → **Generate Android signing keystore** → Run workflow。

Step Summary 会显示：

- 缓存是否命中；
- 证书 SHA-256 指纹；
- 生产环境固定密钥的配置方式。

### 5.3 跳过某次自动发布

不需要操作：只要距上一个 tag 不足 10 次提交，Release 工作流会自动跳过构建。

---

## 6. 工作流权限与安全

- 工作流顶层声明 `permissions: contents: write`，仅用于创建 tag / Release。
- 签名密钥与密码不进入 Git、artifact、release 或日志。
- `Release` 由 `main` 的 push 触发；fork 仓库的 PR 不会获得 Secrets，也不会自动发布。
- 若仓库被设置为“Require approvals for workflows from forks”，可进一步限制。
- Actions 缓存对公众不可见，但协作者可以管理；生产密钥请使用 Secrets。

---

## 7. 常见问题

| 现象 | 原因/处理 |
| --- | --- |
| push 后工作流成功但没有 Release | 距上一个 tag 不足 10 次提交；查看 `version` job 的 Summary |
| Android 构建失败 | 查看 `Build signed Android APK / AAB` 日志；常见原因是 SDK/AGP 版本或 Kotlin 编译错误 |
| 签名指纹每次发布都变 | 缓存被回收或密码变化导致重新生成；按 4.2 固定 Secrets |
| Release 已存在同名 tag | 通常不会发生（版本脚本取最高 tag +1）；如手动创建过 tag，请先确认版本策略 |
| PC 构建失败 | 查看 `Build Windows desktop app` 日志；本机已通过 `cargo build -p rv-pc` 验证 |
| 需要 `0.2.0` | 手动创建并推送 `v0.2.0` tag（或修改版本策略）；下一次发布从该 tag 继续 +1 |
