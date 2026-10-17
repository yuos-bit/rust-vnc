package com.rustvnc.client.data

import android.content.Context
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

enum class ImageQuality(val label: String) {
    Low("流畅"),
    Medium("均衡"),
    High("清晰"),
}

/**
 * Non-secret preferences. Tokens and pre-auth keys live in [SecureStore].
 *
 * Image quality and FPS are local hints only. The v1 control protocol does
 * not negotiate them, so the controlled endpoint keeps using its own capture
 * settings; the values are stored for future protocol revisions and for the
 * user's own reference.
 */
class SettingsStore(context: Context) {
    private val prefs = context.applicationContext
        .getSharedPreferences(FILE_NAME, Context.MODE_PRIVATE)

    private val _serverUrl = MutableStateFlow(prefs.getString(KEY_SERVER_URL, "").orEmpty())
    val serverUrl: StateFlow<String> = _serverUrl.asStateFlow()

    private val _controllerName = MutableStateFlow(
        prefs.getString(KEY_CONTROLLER_NAME, null) ?: DEFAULT_CONTROLLER_NAME,
    )
    val controllerName: StateFlow<String> = _controllerName.asStateFlow()

    private val _controlUrl = MutableStateFlow(prefs.getString(KEY_CONTROL_URL, "").orEmpty())
    val controlUrl: StateFlow<String> = _controlUrl.asStateFlow()

    private val _quality = MutableStateFlow(
        runCatching {
            ImageQuality.valueOf(prefs.getString(KEY_QUALITY, null) ?: ImageQuality.Medium.name)
        }.getOrDefault(ImageQuality.Medium),
    )
    val quality: StateFlow<ImageQuality> = _quality.asStateFlow()

    private val _fps = MutableStateFlow(prefs.getInt(KEY_FPS, DEFAULT_FPS))
    val fps: StateFlow<Int> = _fps.asStateFlow()

    fun setServerUrl(value: String) {
        val normalized = normalizeServerUrl(value)
        prefs.edit().putString(KEY_SERVER_URL, normalized).apply()
        _serverUrl.value = normalized
    }

    fun setControllerName(value: String) {
        val cleaned = value.trim().take(64).ifBlank { DEFAULT_CONTROLLER_NAME }
        prefs.edit().putString(KEY_CONTROLLER_NAME, cleaned).apply()
        _controllerName.value = cleaned
    }

    fun setControlUrl(value: String) {
        val normalized = normalizeServerUrl(value)
        prefs.edit().putString(KEY_CONTROL_URL, normalized).apply()
        _controlUrl.value = normalized
    }

    fun setQuality(value: ImageQuality) {
        prefs.edit().putString(KEY_QUALITY, value.name).apply()
        _quality.value = value
    }

    fun setFps(value: Int) {
        val clamped = nearestAllowedFps(value)
        prefs.edit().putInt(KEY_FPS, clamped).apply()
        _fps.value = clamped
    }

    companion object {
        const val DEFAULT_CONTROLLER_NAME = "Android 控制端"
        const val DEFAULT_FPS = 15
        val allowedFps = listOf(10, 15, 24, 30)

        fun normalizeServerUrl(value: String): String = value.trim().trimEnd('/')

        fun isValidServerUrl(value: String): Boolean {
            val normalized = normalizeServerUrl(value)
            return normalized.startsWith("https://") || normalized.startsWith("http://")
        }

        private const val FILE_NAME = "rvnc_settings"
        private const val KEY_SERVER_URL = "server_url"
        private const val KEY_CONTROLLER_NAME = "controller_name"
        private const val KEY_CONTROL_URL = "tailscale_control_url"
        private const val KEY_QUALITY = "image_quality"
        private const val KEY_FPS = "fps"
    }
}

private fun nearestAllowedFps(value: Int): Int =
    SettingsStore.allowedFps.minByOrNull { kotlin.math.abs(it - value) }
        ?: SettingsStore.DEFAULT_FPS

