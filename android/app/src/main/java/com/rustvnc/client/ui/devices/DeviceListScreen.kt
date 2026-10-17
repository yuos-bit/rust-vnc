package com.rustvnc.client.ui.devices

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
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
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.rustvnc.client.R
import com.rustvnc.client.data.PairingRepository
import com.rustvnc.client.data.SavedDevice
import com.rustvnc.client.ui.SimpleViewModelFactory
import com.rustvnc.client.ui.components.AppBackground
import com.rustvnc.client.ui.components.GlassCard
import com.rustvnc.client.ui.components.GlassIconButton
import com.rustvnc.client.ui.components.GlassSnackbarHost
import com.rustvnc.client.ui.components.GlassTopBar
import com.rustvnc.client.ui.components.NoticeBanner
import com.rustvnc.client.ui.components.PrimaryGlassButton
import com.rustvnc.client.ui.components.StaggeredEntrance
import com.rustvnc.client.ui.components.StatusDot
import com.rustvnc.client.ui.theme.AccentBlue
import com.rustvnc.client.ui.theme.DangerRed
import com.rustvnc.client.ui.theme.FaintText
import com.rustvnc.client.ui.theme.GlassText
import com.rustvnc.client.ui.theme.MutedText
import com.rustvnc.client.ui.theme.OnlineGreen
import com.rustvnc.client.ui.theme.WarningAmber

@Composable
fun DeviceListRoute(
    repository: PairingRepository,
    tailscaleIntegrated: Boolean,
    onAddDevice: () -> Unit,
    onOpenDevice: (String) -> Unit,
    onSettings: () -> Unit,
) {
    val viewModel: DeviceListViewModel = viewModel(
        factory = SimpleViewModelFactory { DeviceListViewModel(repository) },
    )
    val devices by viewModel.devices.collectAsStateWithLifecycle()
    val refreshing by viewModel.refreshing.collectAsStateWithLifecycle()
    val message by viewModel.message.collectAsStateWithLifecycle()
    val serverUrl by repository.serverUrlFlow.collectAsStateWithLifecycle()

    val snackbarHostState = remember { SnackbarHostState() }
    var pendingRemove by remember { mutableStateOf<SavedDevice?>(null) }

    LaunchedEffect(message) {
        val text = message ?: return@LaunchedEffect
        snackbarHostState.showSnackbar(text)
        viewModel.consumeMessage()
    }

    AppBackground {
        Box(Modifier.fillMaxSize()) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .statusBarsPadding()
                    .navigationBarsPadding(),
            ) {
                GlassTopBar(
                    title = "我的设备",
                    subtitle = "Tailscale 虚拟局域网 · 免密连接",
                    onSettings = onSettings,
                    actions = {
                        if (refreshing) {
                            Box(
                                modifier = Modifier.size(40.dp),
                                contentAlignment = Alignment.Center,
                            ) {
                                CircularProgressIndicator(
                                    modifier = Modifier.size(18.dp),
                                    color = AccentBlue,
                                    strokeWidth = 2.dp,
                                )
                            }
                        } else {
                            GlassIconButton(
                                icon = Icons.Filled.Refresh,
                                contentDescription = "刷新设备状态",
                                onClick = viewModel::refresh,
                            )
                        }
                        Spacer(Modifier.width(6.dp))
                        GlassIconButton(
                            icon = Icons.Filled.Add,
                            contentDescription = "配对新设备",
                            onClick = onAddDevice,
                        )
                    },
                )

                Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp)) {
                    if (!tailscaleIntegrated) {
                        NoticeBanner(
                            text = "当前构建未集成 Tailscale 网络层（Noop）。100.x 地址无法访问，仅可直接路由的地址可连接。",
                            color = WarningAmber,
                        )
                        Spacer(Modifier.height(8.dp))
                    }
                    if (serverUrl.isBlank()) {
                        NoticeBanner(
                            text = "尚未配置配对服务器地址，请点击右上角设置。",
                            color = WarningAmber,
                        )
                        Spacer(Modifier.height(8.dp))
                    }
                }

                if (devices.isEmpty()) {
                    Box(
                        modifier = Modifier
                            .fillMaxSize()
                            .padding(16.dp),
                        contentAlignment = Alignment.Center,
                    ) {
                        EmptyState(onAddDevice = onAddDevice)
                    }
                } else {
                    LazyColumn(
                        modifier = Modifier.fillMaxSize(),
                        contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
                        verticalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        itemsIndexed(devices, key = { _, device -> device.deviceCode }) { index, device ->
                            StaggeredEntrance(index = index) {
                                DeviceCard(
                                    device = device,
                                    onOpen = { onOpenDevice(device.deviceCode) },
                                    onRemoveRequest = { pendingRemove = device },
                                )
                            }
                        }
                        item { Spacer(Modifier.height(12.dp)) }
                    }
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

    pendingRemove?.let { device ->
        AlertDialog(
            onDismissRequest = { pendingRemove = null },
            containerColor = Color(0xF21A2440),
            titleContentColor = GlassText,
            textContentColor = MutedText,
            title = { Text("移除设备") },
            text = {
                Text(
                    "将从本机删除 ${device.deviceName}（${device.deviceCode}）的配对令牌。" +
                        "此操作不会在服务器端吊销令牌；如需彻底吊销，请在管理后台操作。",
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        viewModel.removeLocal(device.deviceCode)
                        pendingRemove = null
                    },
                ) {
                    Text("移除", color = DangerRed)
                }
            },
            dismissButton = {
                TextButton(onClick = { pendingRemove = null }) {
                    Text("取消", color = MutedText)
                }
            },
        )
    }
}

@Composable
private fun EmptyState(onAddDevice: () -> Unit) {
    GlassCard(
        modifier = Modifier
            .widthIn(max = 420.dp)
            .fillMaxWidth(),
        padding = PaddingValues(28.dp),
    ) {
        Column(
            modifier = Modifier.fillMaxWidth(),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Icon(
                painter = painterResource(R.drawable.ic_monitor),
                contentDescription = null,
                tint = Color.Unspecified,
                modifier = Modifier.size(52.dp),
            )
            Spacer(Modifier.height(14.dp))
            Text(
                text = "还没有已配对的设备",
                color = GlassText,
                fontSize = 18.sp,
                fontWeight = FontWeight.SemiBold,
            )
            Text(
                text = "在电脑上运行 RustVNC 被控端，输入屏幕显示的 6 位设备码即可开始。",
                color = MutedText,
                fontSize = 12.5.sp,
                textAlign = TextAlign.Center,
                lineHeight = 18.sp,
                modifier = Modifier.padding(top = 8.dp),
            )
            Spacer(Modifier.height(18.dp))
            PrimaryGlassButton(
                text = "配对新设备",
                onClick = onAddDevice,
                modifier = Modifier
                    .widthIn(max = 260.dp)
                    .fillMaxWidth(),
            )
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun DeviceCard(
    device: SavedDevice,
    onOpen: () -> Unit,
    onRemoveRequest: () -> Unit,
) {
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val scale by animateFloatAsState(
        targetValue = if (pressed) 0.98f else 1f,
        animationSpec = tween(100),
        label = "deviceCardScale",
    )
    var menuExpanded by remember { mutableStateOf(false) }

    GlassCard(
        modifier = Modifier
            .fillMaxWidth()
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }
            .combinedClickable(
                interactionSource = interaction,
                indication = null,
                onClick = onOpen,
                onLongClick = onRemoveRequest,
                onLongClickLabel = "移除设备",
            ),
        padding = PaddingValues(horizontal = 14.dp, vertical = 14.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            MonitorTile()
            Spacer(Modifier.width(12.dp))

            Column(Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        text = device.deviceName,
                        color = GlassText,
                        fontSize = 15.sp,
                        fontWeight = FontWeight.Medium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f, fill = false),
                    )
                    Spacer(Modifier.width(8.dp))
                    StatusDot(online = device.online)
                    Spacer(Modifier.width(6.dp))
                    Text(
                        text = if (device.online) "在线" else "离线",
                        color = if (device.online) OnlineGreen else FaintText,
                        fontSize = 11.sp,
                    )
                }
                Spacer(Modifier.height(3.dp))
                Text(
                    text = buildString {
                        append(device.deviceCode)
                        append("   ")
                        append(device.tailscaleIp ?: "地址待解析")
                    },
                    color = MutedText,
                    fontSize = 11.5.sp,
                    fontFamily = FontFamily.Monospace,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }

            Spacer(Modifier.width(4.dp))
            Box {
                GlassIconButton(
                    icon = Icons.Filled.MoreVert,
                    contentDescription = "更多操作",
                    onClick = { menuExpanded = true },
                )
                DropdownMenu(
                    expanded = menuExpanded,
                    onDismissRequest = { menuExpanded = false },
                    containerColor = Color(0xF216213E),
                ) {
                    DropdownMenuItem(
                        text = { Text("立即连接", color = GlassText, fontSize = 13.sp) },
                        onClick = {
                            menuExpanded = false
                            onOpen()
                        },
                    )
                    DropdownMenuItem(
                        text = { Text("移除设备", color = DangerRed, fontSize = 13.sp) },
                        onClick = {
                            menuExpanded = false
                            onRemoveRequest()
                        },
                    )
                }
            }
        }
    }
}

@Composable
private fun MonitorTile() {
    Box(
        modifier = Modifier
            .size(44.dp)
            .clip(RoundedCornerShape(13.dp))
            .background(AccentBlue.copy(alpha = 0.12f))
            .border(1.dp, AccentBlue.copy(alpha = 0.28f), RoundedCornerShape(13.dp)),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            painter = painterResource(R.drawable.ic_monitor),
            contentDescription = null,
            tint = Color.Unspecified,
            modifier = Modifier.size(26.dp),
        )
    }
}
