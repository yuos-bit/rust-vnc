package com.rustvnc.client.ui.settings

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.rustvnc.client.BuildConfig
import com.rustvnc.client.data.ApiClient
import com.rustvnc.client.data.ImageQuality
import com.rustvnc.client.data.SecureStore
import com.rustvnc.client.data.SettingsStore
import com.rustvnc.client.net.TailscaleManager
import com.rustvnc.client.ui.SimpleViewModelFactory
import com.rustvnc.client.ui.components.AppBackground
import com.rustvnc.client.ui.components.GhostGlassButton
import com.rustvnc.client.ui.components.GlassCard
import com.rustvnc.client.ui.components.GlassSegmented
import com.rustvnc.client.ui.components.GlassSnackbarHost
import com.rustvnc.client.ui.components.GlassTextField
import com.rustvnc.client.ui.components.GlassTopBar
import com.rustvnc.client.ui.components.NoticeBanner
import com.rustvnc.client.ui.components.PrimaryGlassButton
import com.rustvnc.client.ui.components.SectionTitle
import com.rustvnc.client.ui.components.StaggeredEntrance
import com.rustvnc.client.ui.theme.AccentBlue
import com.rustvnc.client.ui.theme.DangerRed
import com.rustvnc.client.ui.theme.FaintText
import com.rustvnc.client.ui.theme.GlassText
import com.rustvnc.client.ui.theme.MutedText
import com.rustvnc.client.ui.theme.OnlineGreen
import com.rustvnc.client.ui.theme.WarningAmber
import com.rustvnc.tailscale.TailscaleStatus

@Composable
fun SettingsRoute(
    settings: SettingsStore,
    secure: SecureStore,
    tailscale: TailscaleManager,
    api: ApiClient,
    onBack: () -> Unit,
) {
    val viewModel: SettingsViewModel = viewModel(
        factory = SimpleViewModelFactory { SettingsViewModel(settings, secure, tailscale, api) },
    )
    val state by viewModel.state.collectAsStateWithLifecycle()
    val snackbarHostState = remember { SnackbarHostState() }
    var confirmClear by remember { mutableStateOf(false) }

    LaunchedEffect(state.message) {
        val text = state.message ?: return@LaunchedEffect
        snackbarHostState.showSnackbar(text)
        viewModel.consumeMessage()
    }

    AppBackground {
        Box(Modifier.fillMaxSize()) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .statusBarsPadding()
                    .navigationBarsPadding()
                    .imePadding(),
            ) {
                GlassTopBar(
                    title = "设置",
                    subtitle = "配对、网络与显示",
                    onBack = onBack,
                )

                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .verticalScroll(rememberScrollState())
                        .padding(horizontal = 16.dp, vertical = 8.dp),
                    verticalArrangement = Arrangement.spacedBy(14.dp),
                ) {
                    StaggeredEntrance(index = 0) {
                        ServerCard(
                            serverUrl = state.serverUrl,
                            controllerName = state.controllerName,
                            controllerId = state.controllerId,
                            onServerUrlChange = viewModel::setServerUrl,
                            onControllerNameChange = viewModel::setControllerName,
                            onSave = viewModel::saveGeneral,
                        )
                    }

                    StaggeredEntrance(index = 1) {
                        NetworkCard(
                            state = state,
                            onControlUrlChange = viewModel::setControlUrl,
                            onPreAuthKeyChange = viewModel::setPreAuthKey,
                            onTogglePreAuthVisible = viewModel::togglePreAuthVisible,
                            onEnroll = viewModel::enrollNetwork,
                            onApply = viewModel::applyNetwork,
                            onStop = viewModel::stopNetwork,
                        )
                    }

                    StaggeredEntrance(index = 2) {
                        DisplayCard(
                            quality = state.quality,
                            fps = state.fps,
                            onQualityChange = viewModel::setQuality,
                            onFpsChange = viewModel::setFps,
                        )
                    }

                    StaggeredEntrance(index = 3) {
                        AboutCard(onClearRequest = { confirmClear = true })
                    }

                    Spacer(Modifier.height(18.dp))
                }
            }

            GlassSnackbarHost(
                hostState = snackbarHostState,
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .navigationBarsPadding()
                    .padding(16.dp),
            )
        }
    }

    if (confirmClear) {
        AlertDialog(
            onDismissRequest = { confirmClear = false },
            containerColor = Color(0xF21A2440),
            titleContentColor = GlassText,
            textContentColor = MutedText,
            title = { Text("清除本机配对数据") },
            text = {
                Text(
                    "将删除本机保存的所有设备令牌与地址。服务器端令牌仍然有效；如需彻底吊销，请在管理后台操作。",
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        viewModel.clearLocalPairings()
                        confirmClear = false
                    },
                ) {
                    Text("清除", color = DangerRed)
                }
            },
            dismissButton = {
                TextButton(onClick = { confirmClear = false }) {
                    Text("取消", color = MutedText)
                }
            },
        )
    }
}

@Composable
private fun ServerCard(
    serverUrl: String,
    controllerName: String,
    controllerId: String,
    onServerUrlChange: (String) -> Unit,
    onControllerNameChange: (String) -> Unit,
    onSave: () -> Unit,
) {
    GlassCard(
        modifier = Modifier.fillMaxWidth(),
        padding = PaddingValues(18.dp),
    ) {
        Column {
            SectionTitle("配对服务器")
            Spacer(Modifier.height(12.dp))
            GlassTextField(
                value = serverUrl,
                onValueChange = onServerUrlChange,
                label = "服务器地址",
                placeholder = "https://rvnc.example.com",
                keyboardType = KeyboardType.Uri,
                imeAction = ImeAction.Next,
            )
            Text(
                text = "优先使用 HTTPS。Android 仅允许 localhost 与模拟器使用明文 HTTP。",
                color = FaintText,
                fontSize = 11.sp,
                lineHeight = 16.sp,
                modifier = Modifier.padding(top = 6.dp),
            )

            Spacer(Modifier.height(14.dp))
            GlassTextField(
                value = controllerName,
                onValueChange = onControllerNameChange,
                label = "控制端名称",
                placeholder = SettingsStore.DEFAULT_CONTROLLER_NAME,
                imeAction = ImeAction.Done,
            )

            Spacer(Modifier.height(12.dp))
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(10.dp))
                    .background(Color.Black.copy(alpha = 0.16f))
                    .border(1.dp, Color.White.copy(alpha = 0.08f), RoundedCornerShape(10.dp))
                    .padding(horizontal = 12.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("控制器 ID", color = MutedText, fontSize = 11.5.sp)
                Spacer(Modifier.width(10.dp))
                Text(
                    text = controllerId,
                    color = GlassText.copy(alpha = 0.85f),
                    fontSize = 11.sp,
                    fontFamily = FontFamily.Monospace,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
            }

            Spacer(Modifier.height(16.dp))
            PrimaryGlassButton(
                text = "保存设置",
                onClick = onSave,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

@Composable
private fun NetworkCard(
    state: SettingsViewModel.UiState,
    onControlUrlChange: (String) -> Unit,
    onPreAuthKeyChange: (String) -> Unit,
    onTogglePreAuthVisible: () -> Unit,
    onEnroll: () -> Unit,
    onApply: () -> Unit,
    onStop: () -> Unit,
) {
    val statusColor = when (state.tailscale.status) {
        TailscaleStatus.Unavailable -> WarningAmber
        TailscaleStatus.Stopped -> MutedText
        TailscaleStatus.Starting -> WarningAmber
        TailscaleStatus.Running -> OnlineGreen
        TailscaleStatus.Error -> DangerRed
    }

    GlassCard(
        modifier = Modifier.fillMaxWidth(),
        padding = PaddingValues(18.dp),
    ) {
        Column {
            SectionTitle("网络（Tailscale）")
            Spacer(Modifier.height(10.dp))

            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(
                    modifier = Modifier
                        .size(9.dp)
                        .background(statusColor, CircleShape),
                )
                Spacer(Modifier.width(8.dp))
                Text(
                    text = "网络层：${SettingsViewModel.statusLabel(state.tailscale)}",
                    color = GlassText,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.Medium,
                )
                Spacer(Modifier.weight(1f))
                Text(
                    text = state.tailnetIp ?: "无地址",
                    color = MutedText,
                    fontSize = 11.5.sp,
                    fontFamily = FontFamily.Monospace,
                )
            }

            state.tailscale.message?.let { message ->
                Text(
                    text = message,
                    color = MutedText,
                    fontSize = 11.5.sp,
                    lineHeight = 16.sp,
                    modifier = Modifier.padding(top = 6.dp),
                )
            }

            if (!state.tailscaleIntegrated) {
                Spacer(Modifier.height(10.dp))
                NoticeBanner(
                    text = "官方 Tailscale Android SDK 未随应用分发。当前为 Noop 控制器，接入本地构建的 libtailscale AAR 后此面板才能建立隧道。",
                    color = WarningAmber,
                )
            }

            Spacer(Modifier.height(14.dp))
            GlassTextField(
                value = state.controlUrl,
                onValueChange = onControlUrlChange,
                label = "控制面地址",
                placeholder = "https://headscale.example.com",
                keyboardType = KeyboardType.Uri,
                imeAction = ImeAction.Next,
            )

            Spacer(Modifier.height(12.dp))
            GlassTextField(
                value = state.preAuthKey,
                onValueChange = onPreAuthKeyChange,
                label = "控制器预授权密钥",
                placeholder = "headscale preauthkeys create 生成",
                isPassword = true,
                passwordVisible = state.preAuthVisible,
                onTogglePassword = onTogglePreAuthVisible,
                keyboardType = KeyboardType.Password,
                imeAction = ImeAction.Done,
            )
            Text(
                text = "可点击下方按钮由配对服务器自动签发，也可手动填入 Headscale 密钥。密钥使用 EncryptedSharedPreferences 加密保存在本机。",
                color = FaintText,
                fontSize = 11.sp,
                lineHeight = 16.sp,
                modifier = Modifier.padding(top = 6.dp),
            )

            Spacer(Modifier.height(12.dp))
            GhostGlassButton(
                text = if (state.networkBusy) "正在获取…" else "自动获取网络接入密钥",
                onClick = onEnroll,
                modifier = Modifier.fillMaxWidth(),
                enabled = !state.networkBusy,
            )

            Spacer(Modifier.height(14.dp))
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                GhostGlassButton(
                    text = if (state.networkBusy) "正在启动…" else "应用网络配置",
                    onClick = onApply,
                    modifier = Modifier.weight(1f),
                    enabled = state.tailscaleIntegrated && !state.networkBusy,
                )
                GhostGlassButton(
                    text = "停止",
                    onClick = onStop,
                    modifier = Modifier.weight(1f),
                    enabled = state.tailscaleIntegrated && !state.networkBusy,
                    danger = true,
                )
            }
        }
    }
}

@Composable
private fun DisplayCard(
    quality: ImageQuality,
    fps: Int,
    onQualityChange: (ImageQuality) -> Unit,
    onFpsChange: (Int) -> Unit,
) {
    GlassCard(
        modifier = Modifier.fillMaxWidth(),
        padding = PaddingValues(18.dp),
    ) {
        Column {
            SectionTitle("显示与性能")
            Spacer(Modifier.height(12.dp))

            Text("画面质量", color = GlassText, fontSize = 13.sp)
            Spacer(Modifier.height(8.dp))
            GlassSegmented(
                options = ImageQuality.entries.map { it.label },
                selectedIndex = ImageQuality.entries.indexOf(quality).coerceAtLeast(0),
                onSelected = { index -> onQualityChange(ImageQuality.entries[index]) },
                modifier = Modifier.fillMaxWidth(),
            )

            Spacer(Modifier.height(14.dp))
            Text("目标帧率", color = GlassText, fontSize = 13.sp)
            Spacer(Modifier.height(8.dp))
            GlassSegmented(
                options = SettingsStore.allowedFps.map { "$it fps" },
                selectedIndex = SettingsStore.allowedFps.indexOf(fps).coerceAtLeast(0),
                onSelected = { index -> onFpsChange(SettingsStore.allowedFps[index]) },
                modifier = Modifier.fillMaxWidth(),
            )

            Text(
                text = "以上仅为本机偏好。v1 控制协议不会协商画质与帧率，被控端使用其自身采集配置。",
                color = FaintText,
                fontSize = 11.sp,
                lineHeight = 16.sp,
                modifier = Modifier.padding(top = 10.dp),
            )
        }
    }
}

@Composable
private fun AboutCard(onClearRequest: () -> Unit) {
    GlassCard(
        modifier = Modifier.fillMaxWidth(),
        padding = PaddingValues(18.dp),
    ) {
        Column {
            SectionTitle("关于")
            Spacer(Modifier.height(12.dp))
            InfoRow("应用版本", BuildConfig.VERSION_NAME)
            InfoRow("控制协议", "RVNC v1")
            InfoRow("配对 API", "/api/v1 · v1")
            InfoRow("平台", "android")

            Spacer(Modifier.height(12.dp))
            Text(
                text = "配对令牌与预授权密钥使用 EncryptedSharedPreferences 加密存储，不会写入日志。移除本机设备不会吊销服务器端令牌。",
                color = FaintText,
                fontSize = 11.sp,
                lineHeight = 16.sp,
            )

            Spacer(Modifier.height(14.dp))
            GhostGlassButton(
                text = "清除本机配对数据",
                onClick = onClearRequest,
                modifier = Modifier.fillMaxWidth(),
                danger = true,
            )
        }
    }
}

@Composable
private fun InfoRow(label: String, value: String) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, color = MutedText, fontSize = 12.5.sp)
        Text(
            value,
            color = GlassText.copy(alpha = 0.9f),
            fontSize = 12.5.sp,
            fontFamily = FontFamily.Monospace,
        )
    }
}
