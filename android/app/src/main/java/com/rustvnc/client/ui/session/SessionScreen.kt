package com.rustvnc.client.ui.session

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.view.WindowManager
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.rustvnc.client.data.PairingRepository
import com.rustvnc.client.net.SessionStatus
import com.rustvnc.client.net.TailscaleManager
import com.rustvnc.client.ui.SimpleViewModelFactory
import com.rustvnc.client.ui.components.GhostGlassButton
import com.rustvnc.client.ui.components.GlassCard
import com.rustvnc.client.ui.components.GlassIconButton
import com.rustvnc.client.ui.components.NoticeBanner
import com.rustvnc.client.ui.components.PrimaryGlassButton
import com.rustvnc.client.ui.theme.AccentBlue
import com.rustvnc.client.ui.theme.DangerRed
import com.rustvnc.client.ui.theme.FaintText
import com.rustvnc.client.ui.theme.GlassText
import com.rustvnc.client.ui.theme.MutedText
import com.rustvnc.client.ui.theme.OnlineGreen
import com.rustvnc.client.ui.theme.WarningAmber
import com.rustvnc.client.wire.MouseWireButton
import kotlinx.coroutines.delay
import kotlinx.coroutines.withTimeoutOrNull
import kotlin.math.abs

@OptIn(ExperimentalComposeUiApi::class)
@Composable
fun SessionRoute(
    repository: PairingRepository,
    tailscale: TailscaleManager,
    deviceCode: String,
    onBack: () -> Unit,
    onSettings: () -> Unit,
) {
    val context = LocalContext.current
    val viewModel: SessionViewModel = viewModel(
        key = "session-$deviceCode",
        factory = SimpleViewModelFactory {
            SessionViewModel(context.applicationContext, repository, tailscale, deviceCode)
        },
    )

    val status by viewModel.status.collectAsStateWithLifecycle()
    val frame by viewModel.frame.collectAsStateWithLifecycle()
    val resolving by viewModel.resolving.collectAsStateWithLifecycle()
    val warning by viewModel.warning.collectAsStateWithLifecycle()

    var fullscreen by rememberSaveable { mutableStateOf(false) }
    var confirmDisconnect by remember { mutableStateOf(false) }
    var imeActive by remember { mutableStateOf(false) }

    val keyboard = LocalSoftwareKeyboardController.current
    val focusManager = LocalFocusManager.current
    val imeFocusRequester = remember { FocusRequester() }
    val view = LocalView.current

    KeepScreenOn(context)

    DisposableEffect(fullscreen) {
        val window = context.findActivity()?.window
        if (window != null) {
            val controller = WindowCompat.getInsetsController(window, view)
            if (fullscreen) {
                controller.hide(WindowInsetsCompat.Type.systemBars())
                controller.systemBarsBehavior =
                    WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            } else {
                controller.show(WindowInsetsCompat.Type.systemBars())
            }
        }
        onDispose {
            if (fullscreen && window != null) {
                WindowCompat.getInsetsController(window, view)
                    .show(WindowInsetsCompat.Type.systemBars())
            }
        }
    }

    val connected = status is SessionStatus.Connected
    val displayName = (status as? SessionStatus.Connected)?.hello?.deviceName
        ?: viewModel.device?.deviceName
        ?: "远程设备"

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black),
    ) {
        if (frame != null) {
            val image = remember(frame) { frame!!.asImageBitmap() }
            Image(
                bitmap = image,
                contentDescription = "远程桌面画面",
                contentScale = ContentScale.Fit,
                modifier = Modifier.fillMaxSize(),
            )
        } else if (connected) {
            Text(
                text = "正在接收画面…",
                color = Color.White.copy(alpha = 0.35f),
                fontSize = 12.sp,
                modifier = Modifier.align(Alignment.Center),
            )
        }

        if (connected && frame != null) {
            GestureSurface(
                bitmapWidth = frame!!.width,
                bitmapHeight = frame!!.height,
                onMove = viewModel::move,
                onTap = { x, y -> viewModel.click(x, y, MouseWireButton.Left) },
                onLongPress = { x, y -> viewModel.click(x, y, MouseWireButton.Right) },
                onWheel = viewModel::wheel,
                modifier = Modifier.fillMaxSize(),
            )
        }

        if (!connected) {
            ConnectionOverlay(
                status = status,
                resolving = resolving,
                warning = warning,
                deviceName = displayName,
                onRetry = viewModel::retry,
                onCancel = viewModel::disconnect,
                onBack = onBack,
            )
        }

        SessionToolbar(
            status = status,
            deviceName = displayName,
            warning = warning,
            fullscreen = fullscreen,
            onToggleFullscreen = { fullscreen = !fullscreen },
            onKeyboard = {
                if (imeActive) {
                    keyboard?.hide()
                    focusManager.clearFocus()
                } else {
                    imeFocusRequester.requestFocus()
                    keyboard?.show()
                }
            },
            onDisconnect = { confirmDisconnect = true },
            onSettings = onSettings,
            modifier = Modifier.align(Alignment.TopCenter),
        )

        HiddenImeField(
            focusRequester = imeFocusRequester,
            onFocusChanged = { imeActive = it },
            onText = viewModel::sendText,
            onDelete = { viewModel.sendKey("Backspace") },
            onDone = {
                viewModel.sendKey("Enter")
                focusManager.clearFocus()
                keyboard?.hide()
            },
        )
    }

    if (confirmDisconnect) {
        AlertDialog(
            onDismissRequest = { confirmDisconnect = false },
            containerColor = Color(0xF21A2440),
            titleContentColor = GlassText,
            textContentColor = MutedText,
            title = { Text("断开连接") },
            text = { Text("确定要结束当前远程控制会话吗？") },
            confirmButton = {
                TextButton(
                    onClick = {
                        confirmDisconnect = false
                        viewModel.disconnect()
                    },
                ) {
                    Text("断开", color = DangerRed)
                }
            },
            dismissButton = {
                TextButton(onClick = { confirmDisconnect = false }) {
                    Text("取消", color = MutedText)
                }
            },
        )
    }
}

@Composable
private fun KeepScreenOn(context: Context) {
    DisposableEffect(Unit) {
        val window = context.findActivity()?.window
        window?.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        onDispose {
            window?.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        }
    }
}

@Composable
private fun SessionToolbar(
    status: SessionStatus,
    deviceName: String,
    warning: String?,
    fullscreen: Boolean,
    onToggleFullscreen: () -> Unit,
    onKeyboard: () -> Unit,
    onDisconnect: () -> Unit,
    onSettings: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .statusBarsPadding()
            // Swallow touches that miss the toolbar buttons so they never
            // fall through to the remote desktop gesture layer.
            .pointerInput(Unit) {
                awaitPointerEventScope {
                    while (true) {
                        awaitPointerEvent().changes.forEach { it.consume() }
                    }
                }
            }
            .padding(horizontal = 12.dp, vertical = 8.dp),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .shadow(
                    elevation = 12.dp,
                    shape = RoundedCornerShape(16.dp),
                    clip = false,
                    ambientColor = Color.Black,
                    spotColor = Color.Black,
                )
                .clip(RoundedCornerShape(16.dp))
                .background(Color(0xD9121A30))
                .border(1.dp, Color.White.copy(alpha = 0.16f), RoundedCornerShape(16.dp))
                .padding(horizontal = 10.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(
                    text = deviceName,
                    color = GlassText,
                    fontSize = 14.sp,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    text = statusLine(status),
                    color = statusColor(status),
                    fontSize = 10.5.sp,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }

            ToolbarTextButton(text = "键盘", onClick = onKeyboard)
            ToolbarTextButton(
                text = if (fullscreen) "退出全屏" else "全屏",
                onClick = onToggleFullscreen,
            )
            ToolbarTextButton(text = "断开", onClick = onDisconnect, danger = true)
            Spacer(Modifier.width(4.dp))
            GlassIconButton(
                icon = Icons.Outlined.Settings,
                contentDescription = "设置",
                onClick = onSettings,
                modifier = Modifier.size(36.dp),
            )
        }

        if (warning != null) {
            Spacer(Modifier.height(6.dp))
            NoticeBanner(text = warning, color = WarningAmber)
        }
    }
}

@Composable
private fun ToolbarTextButton(
    text: String,
    onClick: () -> Unit,
    danger: Boolean = false,
) {
    Box(
        modifier = Modifier
            .clip(RoundedCornerShape(9.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 7.dp, vertical = 6.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = text,
            color = if (danger) DangerRed else GlassText.copy(alpha = 0.9f),
            fontSize = 11.5.sp,
        )
    }
}

@Composable
private fun ConnectionOverlay(
    status: SessionStatus,
    resolving: Boolean,
    warning: String?,
    deviceName: String,
    onRetry: () -> Unit,
    onCancel: () -> Unit,
    onBack: () -> Unit,
) {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.45f))
            .padding(24.dp),
        contentAlignment = Alignment.Center,
    ) {
        GlassCard(
            modifier = Modifier
                .widthIn(max = 420.dp)
                .fillMaxWidth(),
            padding = PaddingValues(24.dp),
        ) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                when (status) {
                    SessionStatus.Idle, is SessionStatus.Connecting -> {
                        CircularProgressIndicator(
                            modifier = Modifier.size(34.dp),
                            color = AccentBlue,
                            strokeWidth = 2.dp,
                        )
                        Spacer(Modifier.height(16.dp))
                        Text(
                            text = "正在连接 $deviceName",
                            color = GlassText,
                            fontSize = 17.sp,
                            fontWeight = FontWeight.SemiBold,
                            textAlign = TextAlign.Center,
                        )
                        Text(
                            text = if (resolving) {
                                "正在向配对服务器解析地址…"
                            } else {
                                "正在建立加密控制会话…"
                            },
                            color = MutedText,
                            fontSize = 12.5.sp,
                            textAlign = TextAlign.Center,
                            modifier = Modifier.padding(top = 6.dp),
                        )
                        if (warning != null) {
                            Spacer(Modifier.height(14.dp))
                            NoticeBanner(text = warning, color = WarningAmber)
                        }
                        Spacer(Modifier.height(18.dp))
                        GhostGlassButton(text = "取消", onClick = onCancel)
                    }

                    is SessionStatus.Failed -> {
                        Text(
                            text = "连接失败",
                            color = DangerRed,
                            fontSize = 18.sp,
                            fontWeight = FontWeight.SemiBold,
                        )
                        Text(
                            text = status.message,
                            color = MutedText,
                            fontSize = 12.5.sp,
                            textAlign = TextAlign.Center,
                            lineHeight = 18.sp,
                            modifier = Modifier.padding(top = 8.dp),
                        )
                        if (warning != null) {
                            Spacer(Modifier.height(14.dp))
                            NoticeBanner(text = warning, color = WarningAmber)
                        }
                        Spacer(Modifier.height(18.dp))
                        ActionRow(onRetry = onRetry, onBack = onBack)
                    }

                    is SessionStatus.Disconnected -> {
                        Text(
                            text = "会话已断开",
                            color = GlassText,
                            fontSize = 18.sp,
                            fontWeight = FontWeight.SemiBold,
                        )
                        Text(
                            text = status.reason,
                            color = MutedText,
                            fontSize = 12.5.sp,
                            textAlign = TextAlign.Center,
                            modifier = Modifier.padding(top = 8.dp),
                        )
                        Spacer(Modifier.height(18.dp))
                        ActionRow(onRetry = onRetry, onBack = onBack)
                    }

                    is SessionStatus.Connected -> Unit
                }
            }
        }
    }
}

@Composable
private fun ActionRow(onRetry: () -> Unit, onBack: () -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PrimaryGlassButton(
            text = "重新连接",
            onClick = onRetry,
            modifier = Modifier.weight(1f),
        )
        GhostGlassButton(
            text = "返回",
            onClick = onBack,
            modifier = Modifier.weight(1f),
        )
    }
}

@Composable
private fun HiddenImeField(
    focusRequester: FocusRequester,
    onFocusChanged: (Boolean) -> Unit,
    onText: (String) -> Unit,
    onDelete: () -> Unit,
    onDone: () -> Unit,
) {
    var text by remember { mutableStateOf("") }

    BasicTextField(
        value = text,
        onValueChange = { updated ->
            if (updated.length > text.length) {
                onText(updated.substring(text.length))
            } else if (updated.length < text.length) {
                repeat(text.length - updated.length) { onDelete() }
            }
            // Bound the local buffer; the remote side only needs deltas.
            text = if (updated.length <= 256) updated else updated.takeLast(64)
        },
        modifier = Modifier
            .size(1.dp)
            .alpha(0f)
            .focusRequester(focusRequester)
            .onFocusChanged { onFocusChanged(it.isFocused) },
        textStyle = TextStyle(color = Color.Transparent),
        keyboardOptions = KeyboardOptions(
            keyboardType = KeyboardType.Text,
            imeAction = ImeAction.Done,
        ),
        keyboardActions = KeyboardActions(
            onDone = {
                onDone()
                text = ""
            },
        ),
        singleLine = true,
    )
}

/**
 * Maps touches to normalized coordinates inside the letterboxed image rect.
 * Black bars are ignored. Single tap = left click, long press = right click,
 * two-finger drag = wheel.
 */
@Composable
private fun GestureSurface(
    bitmapWidth: Int,
    bitmapHeight: Int,
    onMove: (Double, Double) -> Unit,
    onTap: (Double, Double) -> Unit,
    onLongPress: (Double, Double) -> Unit,
    onWheel: (Double, Double) -> Unit,
    modifier: Modifier = Modifier,
) {
    val currentMove = rememberUpdatedState(onMove)
    val currentTap = rememberUpdatedState(onTap)
    val currentLongPress = rememberUpdatedState(onLongPress)
    val currentWheel = rememberUpdatedState(onWheel)

    Box(
        modifier = modifier.pointerInput(bitmapWidth, bitmapHeight) {
                if (bitmapWidth <= 0 || bitmapHeight <= 0) return@pointerInput

                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = true)
                    val nodeSize = size
                    val scale = minOf(
                        nodeSize.width.toFloat() / bitmapWidth.toFloat(),
                        nodeSize.height.toFloat() / bitmapHeight.toFloat(),
                    )
                    if (scale <= 0f) return@awaitEachGesture

                    val contentWidth = bitmapWidth * scale
                    val contentHeight = bitmapHeight * scale
                    val left = (nodeSize.width - contentWidth) / 2f
                    val top = (nodeSize.height - contentHeight) / 2f

                    fun normalized(position: Offset): Pair<Double, Double>? {
                        val x = (position.x - left) / contentWidth
                        val y = (position.y - top) / contentHeight
                        if (x !in 0f..1f || y !in 0f..1f) return null
                        return x.toDouble() to y.toDouble()
                    }

                    val longPressTimeout = viewConfiguration.longPressTimeoutMillis
                    val touchSlop = viewConfiguration.touchSlop
                    val startedAt = System.currentTimeMillis()

                    var lastPosition = down.position
                    var moved = false
                    var multiTouch = false
                    var rightClicked = false
                    var lastMulti: Offset? = null
                    var wheelAccum = Offset.Zero
                    var finished = false

                    normalized(down.position)?.let { (x, y) -> currentMove.value(x, y) }

                    while (!finished) {
                        val elapsed = System.currentTimeMillis() - startedAt
                        val remaining = longPressTimeout - elapsed
                        val event = if (!moved && !multiTouch && !rightClicked && remaining > 0) {
                            withTimeoutOrNull(remaining) { awaitPointerEvent() }
                        } else {
                            awaitPointerEvent()
                        }

                        if (event == null) {
                            normalized(lastPosition)?.let { (x, y) ->
                                currentLongPress.value(x, y)
                            }
                            rightClicked = true
                            continue
                        }

                        val pressed = event.changes.filter { it.pressed }
                        when {
                            pressed.size >= 2 -> {
                                multiTouch = true
                                val average = pressed.fold(Offset.Zero) { acc, change ->
                                    acc + change.position
                                } / pressed.size.toFloat()

                                val previous = lastMulti
                                lastMulti = average
                                if (previous != null) {
                                    wheelAccum += average - previous
                                    if (abs(wheelAccum.x) >= WHEEL_STEP_PX ||
                                        abs(wheelAccum.y) >= WHEEL_STEP_PX
                                    ) {
                                        currentWheel.value(
                                            (wheelAccum.x / WHEEL_DIVISOR).toDouble(),
                                            (wheelAccum.y / WHEEL_DIVISOR).toDouble(),
                                        )
                                        wheelAccum = Offset.Zero
                                    }
                                }
                                event.changes.forEach { it.consume() }
                            }

                            pressed.size == 1 -> {
                                val change = pressed.first()
                                if (change.id == down.id) {
                                    val delta = change.position - lastPosition
                                    if (!moved && delta.getDistance() > touchSlop) {
                                        moved = true
                                    }
                                    if (delta.getDistance() > 1f) {
                                        lastPosition = change.position
                                        normalized(change.position)?.let { (x, y) ->
                                            currentMove.value(x, y)
                                        }
                                    }
                                    change.consume()
                                }
                                lastMulti = null
                            }

                            else -> {
                                finished = true
                                if (!multiTouch && !moved && !rightClicked) {
                                    normalized(lastPosition)?.let { (x, y) ->
                                        currentTap.value(x, y)
                                    }
                                }
                            }
                        }
                    }
                }
            },
        ) { }
}

private fun statusLine(status: SessionStatus): String = when (status) {
    is SessionStatus.Connected ->
        "${status.hello.width}×${status.hello.height} · ${status.hello.fps} fps"
    is SessionStatus.Connecting -> "正在连接 ${status.host}:${status.port}"
    is SessionStatus.Failed -> "连接失败"
    is SessionStatus.Disconnected -> "已断开"
    SessionStatus.Idle -> "准备连接…"
}

private fun statusColor(status: SessionStatus): Color = when (status) {
    is SessionStatus.Connected -> OnlineGreen
    is SessionStatus.Connecting -> WarningAmber
    is SessionStatus.Failed -> DangerRed
    is SessionStatus.Disconnected -> FaintText
    SessionStatus.Idle -> MutedText
}

private tailrec fun Context.findActivity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.findActivity()
    else -> null
}

private const val WHEEL_STEP_PX = 20f
private const val WHEEL_DIVISOR = 80f
