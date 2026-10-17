package com.rustvnc.client.ui.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

// Design tokens mirrored from crates/rv-server/web/styles.css.
val BgTop = Color(0xFF1A1A2E)
val BgMid = Color(0xFF16213E)
val BgBottom = Color(0xFF0F3460)

val GlassText = Color(0xFFF4F7FF)
val MutedText = Color(0xB3F4F7FF) // 70%
val FaintText = Color(0x61F4F7FF) // 38%

val AccentBlue = Color(0xFF8EC5FC)
val AccentPurple = Color(0xFFE0C3FC)
val OnlineGreen = Color(0xFF4ADE80)
val WarningAmber = Color(0xFFFBBF24)
val DangerRed = Color(0xFFFF6B6B)

val CardShape = RoundedCornerShape(16.dp)
val LargeCardShape = RoundedCornerShape(22.dp)

private val DarkColors = darkColorScheme(
    primary = AccentBlue,
    onPrimary = Color(0xFF07142A),
    primaryContainer = Color(0xFF16213E),
    onPrimaryContainer = GlassText,
    secondary = AccentPurple,
    onSecondary = Color(0xFF07142A),
    background = BgTop,
    onBackground = GlassText,
    surface = Color(0xFF16213E),
    onSurface = GlassText,
    surfaceVariant = Color(0xFF1B2747),
    onSurfaceVariant = MutedText,
    error = DangerRed,
    onError = Color(0xFF2A0A0A),
    outline = Color(0x26FFFFFF),
)

@Composable
fun RustVncTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = DarkColors,
        typography = Typography(),
        content = content,
    )
}
