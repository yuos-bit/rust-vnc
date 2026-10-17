package com.rustvnc.client.ui.components

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.rustvnc.client.ui.theme.AccentBlue
import com.rustvnc.client.ui.theme.AccentPurple
import com.rustvnc.client.ui.theme.DangerRed
import com.rustvnc.client.ui.theme.FaintText
import com.rustvnc.client.ui.theme.GlassText
import com.rustvnc.client.ui.theme.MutedText
import com.rustvnc.client.ui.theme.WarningAmber

/** Full-width primary action with the accent gradient and a loading spinner. */
@Composable
fun PrimaryGlassButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    loading: Boolean = false,
    enabled: Boolean = true,
) {
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val scale by animateFloatAsState(
        targetValue = if (pressed) 0.97f else 1f,
        animationSpec = tween(90),
        label = "primaryButtonScale",
    )
    val active = enabled && !loading

    Row(
        modifier = modifier
            .height(50.dp)
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }
            .clip(RoundedCornerShape(13.dp))
            .background(
                if (active) {
                    Brush.horizontalGradient(listOf(AccentBlue, AccentPurple))
                } else {
                    Brush.horizontalGradient(
                        listOf(
                            Color.White.copy(alpha = 0.16f),
                            Color.White.copy(alpha = 0.10f),
                        ),
                    )
                },
            )
            .border(1.dp, Color.White.copy(alpha = if (active) 0.55f else 0.18f), RoundedCornerShape(13.dp))
            .clickable(
                interactionSource = interaction,
                indication = null,
                enabled = active,
                role = Role.Button,
                onClick = onClick,
            )
            .padding(horizontal = 18.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
    ) {
        if (loading) {
            CircularProgressIndicator(
                modifier = Modifier.size(18.dp),
                color = Color(0xFF07142A),
                strokeWidth = 2.dp,
            )
            Spacer(Modifier.width(10.dp))
        }
        Text(
            text = text,
            color = if (active) Color(0xFF07142A) else Color.White.copy(alpha = 0.5f),
            fontSize = 15.sp,
            fontWeight = FontWeight.SemiBold,
        )
    }
}

/** Secondary action used for cancel/retry/network controls. */
@Composable
fun GhostGlassButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    danger: Boolean = false,
) {
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val scale by animateFloatAsState(
        targetValue = if (pressed) 0.97f else 1f,
        animationSpec = tween(90),
        label = "ghostButtonScale",
    )
    val contentColor = when {
        !enabled -> Color.White.copy(alpha = 0.3f)
        danger -> DangerRed.copy(alpha = 0.92f)
        else -> GlassText
    }

    Row(
        modifier = modifier
            .height(44.dp)
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }
            .clip(RoundedCornerShape(12.dp))
            .background(
                if (danger) DangerRed.copy(alpha = 0.09f) else Color.White.copy(alpha = 0.06f),
            )
            .border(
                1.dp,
                if (danger) DangerRed.copy(alpha = 0.30f) else Color.White.copy(alpha = 0.16f),
                RoundedCornerShape(12.dp),
            )
            .clickable(
                interactionSource = interaction,
                indication = null,
                enabled = enabled,
                role = Role.Button,
                onClick = onClick,
            )
            .padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
    ) {
        Text(text = text, color = contentColor, fontSize = 13.5.sp)
    }
}

/**
 * Glass text field used by the pairing and settings screens.
 * The password toggle is a text button to avoid relying on extended icons.
 */
@Composable
fun GlassTextField(
    value: String,
    onValueChange: (String) -> Unit,
    label: String,
    modifier: Modifier = Modifier,
    placeholder: String? = null,
    isPassword: Boolean = false,
    passwordVisible: Boolean = false,
    onTogglePassword: (() -> Unit)? = null,
    keyboardType: KeyboardType = KeyboardType.Text,
    imeAction: ImeAction = ImeAction.Next,
    enabled: Boolean = true,
) {
    val animation: VisualTransformation =
        if (isPassword && !passwordVisible) PasswordVisualTransformation() else VisualTransformation.None

    val trailing: (@Composable () -> Unit)? =
        if (isPassword && onTogglePassword != null) {
            {
                TextButton(onClick = onTogglePassword) {
                    Text(
                        text = if (passwordVisible) "隐藏" else "显示",
                        color = AccentBlue,
                        fontSize = 12.sp,
                    )
                }
            }
        } else {
            null
        }

    val placeholderContent: (@Composable () -> Unit)? =
        if (placeholder != null) {
            { Text(placeholder, fontSize = 13.sp) }
        } else {
            null
        }

    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        modifier = modifier.fillMaxWidth(),
        enabled = enabled,
        singleLine = true,
        label = { Text(label, fontSize = 12.5.sp) },
        placeholder = placeholderContent,
        visualTransformation = animation,
        keyboardOptions = KeyboardOptions(keyboardType = keyboardType, imeAction = imeAction),
        trailingIcon = trailing,
        shape = RoundedCornerShape(12.dp),
        colors = OutlinedTextFieldDefaults.colors(
            focusedTextColor = GlassText,
            unfocusedTextColor = GlassText,
            disabledTextColor = GlassText.copy(alpha = 0.4f),
            focusedContainerColor = Color.Black.copy(alpha = 0.20f),
            unfocusedContainerColor = Color.Black.copy(alpha = 0.16f),
            disabledContainerColor = Color.Black.copy(alpha = 0.10f),
            cursorColor = AccentBlue,
            focusedBorderColor = AccentBlue.copy(alpha = 0.65f),
            unfocusedBorderColor = Color.White.copy(alpha = 0.12f),
            disabledBorderColor = Color.White.copy(alpha = 0.08f),
            focusedLabelColor = AccentBlue,
            unfocusedLabelColor = MutedText,
            focusedPlaceholderColor = FaintText,
            unfocusedPlaceholderColor = FaintText,
        ),
    )
}

/** Segmented control used for quality/FPS hints. */
@Composable
fun GlassSegmented(
    options: List<String>,
    selectedIndex: Int,
    onSelected: (Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .clip(RoundedCornerShape(12.dp))
            .background(Color.White.copy(alpha = 0.05f))
            .border(1.dp, Color.White.copy(alpha = 0.09f), RoundedCornerShape(12.dp))
            .padding(4.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        options.forEachIndexed { index, label ->
            val active = index == selectedIndex
            Box(
                modifier = Modifier
                    .weight(1f)
                    .clip(RoundedCornerShape(9.dp))
                    .background(if (active) Color.White.copy(alpha = 0.14f) else Color.Transparent)
                    .clickable(role = Role.Button) { onSelected(index) }
                    .padding(vertical = 8.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    text = label,
                    color = if (active) GlassText else MutedText,
                    fontSize = 12.5.sp,
                    fontWeight = if (active) FontWeight.Medium else FontWeight.Normal,
                )
            }
        }
    }
}

@Composable
fun SectionTitle(text: String) {
    Text(
        text = text,
        color = MutedText,
        fontSize = 12.sp,
        letterSpacing = 0.6.sp,
        fontWeight = FontWeight.Medium,
    )
}

/** Inline notice used for the Tailscale integration warning and errors. */
@Composable
fun NoticeBanner(
    text: String,
    modifier: Modifier = Modifier,
    color: Color = WarningAmber,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(color.copy(alpha = 0.10f))
            .border(1.dp, color.copy(alpha = 0.28f), RoundedCornerShape(12.dp))
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier
                .size(7.dp)
                .background(color, CircleShape),
        )
        Spacer(Modifier.width(9.dp))
        Text(
            text = text,
            color = color.copy(alpha = 0.95f),
            fontSize = 12.sp,
            lineHeight = 17.sp,
        )
    }
}
