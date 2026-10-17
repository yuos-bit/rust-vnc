package com.rustvnc.client.ui.components

import android.graphics.RenderEffect
import android.graphics.Shader
import android.os.Build
import androidx.annotation.RequiresApi
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material3.Icon
import androidx.compose.material3.Snackbar
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.asComposeRenderEffect
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.rustvnc.client.R
import com.rustvnc.client.ui.theme.AccentBlue
import com.rustvnc.client.ui.theme.BgBottom
import com.rustvnc.client.ui.theme.BgMid
import com.rustvnc.client.ui.theme.BgTop
import com.rustvnc.client.ui.theme.GlassText
import com.rustvnc.client.ui.theme.MutedText
import com.rustvnc.client.ui.theme.OnlineGreen
import kotlinx.coroutines.delay

/**
 * True backdrop blur is not available to a Compose view. This modifier uses
 * RenderEffect on Android 12+ to blur the layer it is applied to (used for
 * the ambient orbs). Cards use translucent fills + bright borders as the
 * platform-appropriate glassmorphism approximation; the fallback on older
 * versions is the same soft radial gradient without extra blur.
 */
fun Modifier.platformBlur(radius: Dp): Modifier =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) blurLayer(radius) else this

@RequiresApi(Build.VERSION_CODES.S)
private fun Modifier.blurLayer(radius: Dp): Modifier = graphicsLayer {
    renderEffect = RenderEffect.createBlurEffect(
        radius.toPx(),
        radius.toPx(),
        Shader.TileMode.CLAMP,
    ).asComposeRenderEffect()
}

/** Three-stop gradient plus drifting, blurred color orbs. */
@Composable
fun AppBackground(
    modifier: Modifier = Modifier,
    content: @Composable BoxScope.() -> Unit,
) {
    Box(
        modifier
            .fillMaxSize()
            .background(Brush.linearGradient(listOf(BgTop, BgMid, BgBottom))),
    ) {
        Orb(
            modifier = Modifier
                .align(Alignment.TopStart)
                .offset(x = (-120).dp, y = (-170).dp),
            size = 460,
            color = Color(0xFF6A5CFF),
        )
        Orb(
            modifier = Modifier
                .align(Alignment.TopEnd)
                .offset(x = 190.dp, y = 220.dp),
            size = 520,
            color = Color(0xFF2F8FE0),
        )
        Orb(
            modifier = Modifier
                .align(Alignment.BottomStart)
                .offset(x = 120.dp, y = 150.dp),
            size = 380,
            color = Color(0xFFD67AD6),
        )
        content()
    }
}

@Composable
private fun Orb(
    modifier: Modifier,
    size: Int,
    color: Color,
) {
    Box(
        modifier
            .size(size.dp)
            .platformBlur(70.dp)
            .background(
                brush = Brush.radialGradient(
                    colors = listOf(color.copy(alpha = 0.55f), Color.Transparent),
                    center = Offset.Unspecified,
                    radius = Float.POSITIVE_INFINITY,
                ),
                shape = CircleShape,
            ),
    )
}

/** Translucent card with a bright hairline border and soft shadow. */
@Composable
fun GlassCard(
    modifier: Modifier = Modifier,
    shape: Shape = RoundedCornerShape(16.dp),
    padding: PaddingValues = PaddingValues(20.dp),
    content: @Composable BoxScope.() -> Unit,
) {
    Box(
        modifier
            .shadow(
                elevation = 16.dp,
                shape = shape,
                clip = false,
                ambientColor = Color.Black,
                spotColor = Color.Black,
            )
            .clip(shape)
            .background(
                Brush.linearGradient(
                    listOf(
                        Color.White.copy(alpha = 0.11f),
                        Color.White.copy(alpha = 0.055f),
                    ),
                ),
            )
            .border(1.dp, Color.White.copy(alpha = 0.15f), shape)
            .padding(padding),
        content = content,
    )
}

@Composable
fun GlassIconButton(
    icon: ImageVector,
    contentDescription: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val scale by animateFloatAsState(
        targetValue = if (pressed) 0.92f else 1f,
        animationSpec = tween(durationMillis = 80),
        label = "glassIconScale",
    )

    Box(
        modifier
            .size(40.dp)
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }
            .clip(CircleShape)
            .background(Color.White.copy(alpha = 0.08f))
            .border(1.dp, Color.White.copy(alpha = 0.15f), CircleShape)
            .clickable(
                interactionSource = interaction,
                indication = null,
                enabled = enabled,
                role = Role.Button,
                onClick = onClick,
            ),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            imageVector = icon,
            contentDescription = contentDescription,
            tint = Color.White.copy(alpha = if (enabled) 0.8f else 0.3f),
            modifier = Modifier.size(21.dp),
        )
    }
}

/**
 * Floating glass navigation bar. [onSettings] renders the required top-right
 * gear on main screens; [onBack] replaces the brand tile on sub-screens.
 */
@Composable
fun GlassTopBar(
    title: String,
    subtitle: String? = null,
    onBack: (() -> Unit)? = null,
    onSettings: (() -> Unit)? = null,
    actions: @Composable RowScope.() -> Unit = {},
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 10.dp)
            .shadow(
                elevation = 12.dp,
                shape = RoundedCornerShape(18.dp),
                clip = false,
                ambientColor = Color.Black,
                spotColor = Color.Black,
            )
            .clip(RoundedCornerShape(18.dp))
            .background(
                Brush.linearGradient(
                    listOf(
                        Color.White.copy(alpha = 0.10f),
                        Color.White.copy(alpha = 0.055f),
                    ),
                ),
            )
            .border(1.dp, Color.White.copy(alpha = 0.14f), RoundedCornerShape(18.dp))
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (onBack != null) {
            GlassIconButton(
                icon = Icons.AutoMirrored.Filled.ArrowBack,
                contentDescription = "返回",
                onClick = onBack,
            )
            Spacer(Modifier.width(8.dp))
        } else {
            LogoTile()
            Spacer(Modifier.width(10.dp))
        }

        Column(Modifier.weight(1f)) {
            Text(
                text = title,
                color = GlassText,
                fontSize = 18.sp,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (subtitle != null) {
                Text(
                    text = subtitle,
                    color = MutedText,
                    fontSize = 11.sp,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }

        actions()

        if (onSettings != null) {
            Spacer(Modifier.width(6.dp))
            GlassIconButton(
                icon = Icons.Outlined.Settings,
                contentDescription = "设置",
                onClick = onSettings,
            )
        }
    }
}

@Composable
private fun LogoTile() {
    Box(
        modifier = Modifier
            .size(38.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(AccentBlue.copy(alpha = 0.12f))
            .border(1.dp, AccentBlue.copy(alpha = 0.28f), RoundedCornerShape(12.dp)),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            painter = painterResource(R.drawable.ic_monitor),
            contentDescription = null,
            tint = Color.Unspecified,
            modifier = Modifier.size(24.dp),
        )
    }
}

@Composable
fun GlassSnackbarHost(
    hostState: SnackbarHostState,
    modifier: Modifier = Modifier,
) {
    SnackbarHost(hostState = hostState, modifier = modifier) { data ->
        Snackbar(
            snackbarData = data,
            shape = RoundedCornerShape(12.dp),
            containerColor = Color(0xF2151B30),
            contentColor = GlassText,
        )
    }
}

/**
 * Staggered card entrance: translateY + fade, 50 ms per list position.
 */
@Composable
fun StaggeredEntrance(
    index: Int,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    var visible by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        delay((index.coerceIn(0, 8) * 50).toLong())
        visible = true
    }

    AnimatedVisibility(
        visible = visible,
        modifier = modifier,
        enter = fadeIn(tween(320)) + slideInVertically(tween(320)) { height -> height / 6 },
        exit = fadeOut(tween(120)),
    ) {
        content()
    }
}

@Composable
fun StatusDot(
    online: Boolean,
    modifier: Modifier = Modifier,
) {
    if (online) {
        val transition = rememberInfiniteTransition(label = "onlinePulse")
        val alpha by transition.animateFloat(
            initialValue = 0.45f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(
                animation = tween(1200),
                repeatMode = RepeatMode.Reverse,
            ),
            label = "onlineAlpha",
        )
        Box(
            modifier = modifier
                .size(10.dp)
                .background(OnlineGreen.copy(alpha = alpha), CircleShape),
        )
    } else {
        Box(
            modifier = modifier
                .size(10.dp)
                .background(Color.White.copy(alpha = 0.25f), CircleShape),
        )
    }
}
