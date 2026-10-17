package com.rustvnc.client.ui.pairing

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusManager
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.rustvnc.client.data.DeviceCode
import com.rustvnc.client.data.PairingRepository
import com.rustvnc.client.ui.SimpleViewModelFactory
import com.rustvnc.client.ui.components.AppBackground
import com.rustvnc.client.ui.components.GhostGlassButton
import com.rustvnc.client.ui.components.GlassCard
import com.rustvnc.client.ui.components.GlassTextField
import com.rustvnc.client.ui.components.GlassTopBar
import com.rustvnc.client.ui.components.NoticeBanner
import com.rustvnc.client.ui.components.PrimaryGlassButton
import com.rustvnc.client.ui.theme.AccentBlue
import com.rustvnc.client.ui.theme.DangerRed
import com.rustvnc.client.ui.theme.FaintText
import com.rustvnc.client.ui.theme.GlassText
import com.rustvnc.client.ui.theme.MutedText
import com.rustvnc.client.ui.theme.WarningAmber

@Composable
fun PairingRoute(
    repository: PairingRepository,
    onBack: () -> Unit,
    onSettings: () -> Unit,
    onPaired: (String?) -> Unit,
) {
    val viewModel: PairingViewModel = viewModel(
        factory = SimpleViewModelFactory { PairingViewModel(repository) },
    )
    val state by viewModel.state.collectAsStateWithLifecycle()
    val serverUrl by viewModel.serverUrl.collectAsStateWithLifecycle()
    val paired by viewModel.paired.collectAsStateWithLifecycle()

    LaunchedEffect(paired) {
        val outcome = paired ?: return@LaunchedEffect
        viewModel.consumePaired()
        onPaired(outcome.warning)
    }

    AppBackground {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .statusBarsPadding()
                .navigationBarsPadding()
                .imePadding(),
        ) {
            GlassTopBar(
                title = "配对设备",
                subtitle = "6 位设备码 + 密码",
                onBack = onBack,
                onSettings = onSettings,
            )

            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Spacer(Modifier.height(10.dp))
                Text(
                    text = "连接一台新的电脑",
                    color = GlassText,
                    fontSize = 24.sp,
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    text = "输入被控端屏幕上显示的设备码与配对密码",
                    color = MutedText,
                    fontSize = 13.sp,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.padding(top = 6.dp, bottom = 18.dp),
                )

                if (serverUrl.isBlank()) {
                    NoticeBanner(
                        text = "尚未配置配对服务器地址，请先在设置中填写。",
                        color = WarningAmber,
                        modifier = Modifier
                            .widthIn(max = 420.dp)
                            .fillMaxWidth()
                            .padding(bottom = 12.dp),
                    )
                }

                GlassCard(
                    modifier = Modifier
                        .widthIn(max = 420.dp)
                        .fillMaxWidth(),
                    padding = PaddingValues(20.dp),
                ) {
                    Column {
                        Text(
                            text = "设备码",
                            color = GlassText,
                            fontSize = 14.sp,
                            fontWeight = FontWeight.Medium,
                        )
                        Text(
                            text = "共 6 位，字母不区分大小写",
                            color = FaintText,
                            fontSize = 11.5.sp,
                            modifier = Modifier.padding(top = 2.dp, bottom = 10.dp),
                        )

                        CodeInputRow(
                            cells = state.cells,
                            onCells = viewModel::updateCells,
                        )

                        Spacer(Modifier.height(18.dp))

                        GlassTextField(
                            value = state.password,
                            onValueChange = viewModel::setPassword,
                            label = "配对密码",
                            placeholder = "输入被控端显示的密码",
                            isPassword = true,
                            passwordVisible = state.passwordVisible,
                            onTogglePassword = viewModel::togglePasswordVisible,
                            keyboardType = KeyboardType.Password,
                            imeAction = ImeAction.Done,
                            enabled = !state.loading,
                        )

                        AnimatedVisibility(
                            visible = state.error != null,
                            enter = fadeIn(tween(160)) + expandVertically(tween(180)),
                            exit = fadeOut(tween(120)) + shrinkVertically(tween(140)),
                        ) {
                            Box(Modifier.padding(top = 12.dp)) {
                                NoticeBanner(
                                    text = state.error.orEmpty(),
                                    color = DangerRed,
                                )
                            }
                        }

                        Spacer(Modifier.height(18.dp))

                        PrimaryGlassButton(
                            text = if (state.loading) "配对中…" else "配对",
                            onClick = viewModel::pair,
                            modifier = Modifier.fillMaxWidth(),
                            loading = state.loading,
                            enabled = state.complete && state.password.isNotEmpty(),
                        )
                    }
                }

                Spacer(Modifier.height(14.dp))
                Text(
                    text = "配对成功后，令牌会加密保存在本机，后续连接无需再次输入密码。建议使用 HTTPS 服务器地址。",
                    color = FaintText,
                    fontSize = 11.5.sp,
                    textAlign = TextAlign.Center,
                    lineHeight = 17.sp,
                    modifier = Modifier.padding(horizontal = 8.dp),
                )

                if (serverUrl.isBlank()) {
                    Spacer(Modifier.height(10.dp))
                    GhostGlassButton(
                        text = "前往设置填写服务器地址",
                        onClick = onSettings,
                    )
                }

                Spacer(Modifier.height(24.dp))
            }
        }
    }
}

@Composable
private fun CodeInputRow(
    cells: List<Char?>,
    onCells: (List<Char?>) -> Unit,
) {
    val focusRequesters = remember {
        List(DeviceCode.LENGTH) { FocusRequester() }
    }
    val focusManager = LocalFocusManager.current
    val focusedStates = remember {
        List(DeviceCode.LENGTH) { mutableStateOf(false) }
    }
    val scrollState = rememberScrollState()

    Box(
        modifier = Modifier.fillMaxWidth(),
        contentAlignment = Alignment.Center,
    ) {
        Row(
            modifier = Modifier.horizontalScroll(scrollState),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            repeat(DeviceCode.LENGTH) { index ->
                CodeCell(
                    index = index,
                    cell = cells.getOrNull(index),
                    focused = focusedStates[index].value,
                    onFocusChanged = { focusedStates[index].value = it },
                    onInput = { raw ->
                        handleCellInput(
                            index = index,
                            raw = raw,
                            cells = cells,
                            onCells = onCells,
                            focusRequesters = focusRequesters,
                            focusManager = focusManager,
                        )
                    },
                    onBackspaceOnEmpty = {
                        if (index > 0) {
                            val next = cells.toMutableList()
                            next[index - 1] = null
                            onCells(next)
                            focusRequesters[index - 1].requestFocus()
                        }
                    },
                    onImeNext = {
                        if (index < DeviceCode.LENGTH - 1) {
                            focusRequesters[index + 1].requestFocus()
                        } else {
                            focusManager.clearFocus()
                        }
                    },
                    focusRequester = focusRequesters[index],
                    isLast = index == DeviceCode.LENGTH - 1,
                )
            }
        }
    }
}

@Composable
private fun CodeCell(
    index: Int,
    cell: Char?,
    focused: Boolean,
    onFocusChanged: (Boolean) -> Unit,
    onInput: (String) -> Unit,
    onBackspaceOnEmpty: () -> Unit,
    onImeNext: () -> Unit,
    focusRequester: FocusRequester,
    isLast: Boolean,
) {
    val filled = cell != null
    val borderColor by animateColorAsState(
        targetValue = when {
            focused -> AccentBlue.copy(alpha = 0.85f)
            filled -> Color.White.copy(alpha = 0.30f)
            else -> Color.White.copy(alpha = 0.14f)
        },
        animationSpec = tween(150),
        label = "codeCellBorder$index",
    )
    val scale by animateFloatAsState(
        targetValue = if (focused) 1.05f else 1f,
        animationSpec = spring(
            dampingRatio = Spring.DampingRatioMediumBouncy,
            stiffness = Spring.StiffnessMediumLow,
        ),
        label = "codeCellScale$index",
    )

    Box(
        modifier = Modifier
            .width(48.dp)
            .height(56.dp)
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }
            .clip(RoundedCornerShape(12.dp))
            .background(Color.Black.copy(alpha = if (filled) 0.26f else 0.18f))
            .border(1.dp, borderColor, RoundedCornerShape(12.dp)),
        contentAlignment = Alignment.Center,
    ) {
        BasicTextField(
            value = cell?.toString().orEmpty(),
            onValueChange = onInput,
            modifier = Modifier
                .fillMaxSize()
                .padding(4.dp)
                .focusRequester(focusRequester)
                .onFocusChanged { onFocusChanged(it.isFocused) }
                .onPreviewKeyEvent { event ->
                    val backspace = event.type == KeyEventType.KeyDown &&
                        event.key == Key.Backspace &&
                        cell == null
                    if (backspace) {
                        onBackspaceOnEmpty()
                        true
                    } else {
                        false
                    }
                },
            textStyle = TextStyle(
                color = GlassText,
                fontSize = 24.sp,
                fontFamily = FontFamily.Monospace,
                fontWeight = FontWeight.Medium,
                textAlign = TextAlign.Center,
            ),
            keyboardOptions = KeyboardOptions(
                keyboardType = KeyboardType.Ascii,
                imeAction = if (isLast) ImeAction.Done else ImeAction.Next,
            ),
            keyboardActions = KeyboardActions(
                onNext = { onImeNext() },
                onDone = { onImeNext() },
            ),
            singleLine = true,
            cursorBrush = SolidColor(AccentBlue),
        )
    }
}

/**
 * Handles typing, replacement, paste and deletion for one code box.
 * Valid characters are restricted to the server's unambiguous alphabet.
 */
private fun handleCellInput(
    index: Int,
    raw: String,
    cells: List<Char?>,
    onCells: (List<Char?>) -> Unit,
    focusRequesters: List<FocusRequester>,
    focusManager: FocusManager,
) {
    val typedAll = raw.mapNotNull(DeviceCode::sanitizeChar)
    val old = cells.getOrNull(index)

    if (typedAll.isEmpty()) {
        val next = cells.toMutableList()
        next[index] = null
        onCells(next)
        return
    }

    // A non-empty BasicTextField keeps its character in `raw`; drop it so a
    // replacement or paste starts from the newly supplied characters.
    val typed = if (old != null && typedAll.firstOrNull() == old) typedAll.drop(1) else typedAll
    if (typed.isEmpty()) {
        val next = cells.toMutableList()
        next[index] = null
        onCells(next)
        return
    }

    if (typed.size == 1) {
        val next = cells.toMutableList()
        next[index] = typed[0]
        onCells(next)
        if (index < DeviceCode.LENGTH - 1) {
            focusRequesters[index + 1].requestFocus()
        } else {
            focusManager.clearFocus()
        }
        return
    }

    val next = cells.toMutableList()
    val limit = minOf(typed.size, DeviceCode.LENGTH - index)
    for (offset in 0 until limit) {
        next[index + offset] = typed[offset]
    }
    onCells(next)

    val target = index + limit
    if (target < DeviceCode.LENGTH) {
        focusRequesters[target].requestFocus()
    } else {
        focusManager.clearFocus()
    }
}
