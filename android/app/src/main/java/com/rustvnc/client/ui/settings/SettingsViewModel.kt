package com.rustvnc.client.ui.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import android.os.Build
import com.rustvnc.client.data.ApiClient
import com.rustvnc.client.data.EmbeddedEnv
import com.rustvnc.client.data.EnrollControllerRequestDto
import com.rustvnc.client.data.ImageQuality
import com.rustvnc.client.data.SecureStore
import com.rustvnc.client.data.SettingsStore
import com.rustvnc.client.net.TailscaleManager
import com.rustvnc.tailscale.TailscaleState
import com.rustvnc.tailscale.TailscaleStatus
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

class SettingsViewModel(
    private val settings: SettingsStore,
    private val secure: SecureStore,
    private val tailscale: TailscaleManager,
    private val api: ApiClient,
) : ViewModel() {

    data class UiState(
        val serverUrl: String = "",
        val controllerName: String = "",
        val controllerId: String = "",
        val controlUrl: String = "",
        val preAuthKey: String = "",
        val preAuthVisible: Boolean = false,
        val quality: ImageQuality = ImageQuality.Medium,
        val fps: Int = SettingsStore.DEFAULT_FPS,
        val tailscale: TailscaleState = TailscaleState(),
        val tailscaleIntegrated: Boolean = false,
        val tailnetIp: String? = null,
        val networkBusy: Boolean = false,
        val message: String? = null,
    )

    private val _state = MutableStateFlow(
        UiState(
            serverUrl = settings.serverUrl.value,
            controllerName = settings.controllerName.value,
            controllerId = secure.controllerId,
            controlUrl = settings.controlUrl.value.ifBlank {
                EmbeddedEnv.value("headscale服务器地址", "HEADSCALE_URL", "TS_CONTROLURL") ?: ""
            },
            preAuthKey = secure.preAuthKey().orEmpty().ifBlank {
                EmbeddedEnv.value("headscale-key", "HEADSCALE_KEY", "TS_AUTHKEY") ?: ""
            },
            quality = settings.quality.value,
            fps = settings.fps.value,
            tailscale = tailscale.state.value,
            tailscaleIntegrated = tailscale.isIntegrated,
            tailnetIp = tailscale.tailnetIp(),
        ),
    )
    val state: StateFlow<UiState> = _state.asStateFlow()

    init {
        viewModelScope.launch {
            tailscale.state.collect { value ->
                _state.update {
                    it.copy(
                        tailscale = value,
                        tailnetIp = tailscale.tailnetIp(),
                    )
                }
            }
        }
    }

    fun setServerUrl(value: String) {
        _state.update { it.copy(serverUrl = value, message = null) }
    }

    fun setControllerName(value: String) {
        _state.update { it.copy(controllerName = value, message = null) }
    }

    fun setControlUrl(value: String) {
        _state.update { it.copy(controlUrl = value, message = null) }
    }

    fun setPreAuthKey(value: String) {
        _state.update { it.copy(preAuthKey = value, message = null) }
    }

    fun togglePreAuthVisible() {
        _state.update { it.copy(preAuthVisible = !it.preAuthVisible) }
    }

    fun setQuality(value: ImageQuality) {
        settings.setQuality(value)
        _state.update { it.copy(quality = value) }
    }

    fun setFps(value: Int) {
        settings.setFps(value)
        _state.update { it.copy(fps = settings.fps.value) }
    }

    fun saveGeneral() {
        val snapshot = _state.value
        val rawServer = snapshot.serverUrl.trim()
        if (rawServer.isNotEmpty() && !SettingsStore.isValidServerUrl(rawServer)) {
            _state.update { it.copy(message = "服务器地址必须以 http:// 或 https:// 开头") }
            return
        }

        settings.setServerUrl(rawServer)
        settings.setControllerName(snapshot.controllerName)

        val note = if (settings.serverUrl.value.startsWith("http://")) {
            "已保存。注意：除 localhost 与模拟器外，Android 会阻止明文 HTTP，请使用 HTTPS。"
        } else {
            "设置已保存"
        }
        _state.update {
            it.copy(
                serverUrl = settings.serverUrl.value,
                controllerName = settings.controllerName.value,
                message = note,
            )
        }
    }

    /**
     * Requests a single-use tailnet key from the pairing backend, then stores
     * it encrypted and exposes it in the form so the operator can apply it.
     */
    fun enrollNetwork() {
        val snapshot = _state.value
        if (!SettingsStore.isValidServerUrl(snapshot.serverUrl)) {
            _state.update { it.copy(message = "请先填写有效的配对服务器地址") }
            return
        }
        _state.update { it.copy(networkBusy = true, message = null) }
        viewModelScope.launch {
            try {
                val response = api.enrollController(
                    serverUrl = settings.serverUrl.value,
                    request = EnrollControllerRequestDto(
                        name = snapshot.controllerName.ifBlank { "Android" },
                        fingerprint = Build.MODEL ?: "android",
                        controllerId = snapshot.controllerId,
                    ),
                )
                settings.setControlUrl(response.controlUrl)
                secure.savePreAuthKey(response.preauthKey)
                _state.update {
                    it.copy(
                        networkBusy = false,
                        controlUrl = response.controlUrl,
                        preAuthKey = response.preauthKey,
                        preAuthVisible = true,
                        message = "已获取网络接入密钥（主机名 ${response.hostname}）",
                    )
                }
            } catch (error: Exception) {
                _state.update {
                    it.copy(
                        networkBusy = false,
                        message = error.message ?: "获取网络接入密钥失败",
                    )
                }
            }
        }
    }

    fun applyNetwork() {        val snapshot = _state.value
        if (!snapshot.tailscaleIntegrated) {
            _state.update {
                it.copy(message = "当前构建未集成 Tailscale 网络层（Noop 控制器），无法启动隧道。")
            }
            return
        }
        if (!SettingsStore.isValidServerUrl(snapshot.controlUrl.trim())) {
            _state.update { it.copy(message = "控制面地址必须以 http:// 或 https:// 开头") }
            return
        }
        if (snapshot.preAuthKey.isBlank()) {
            _state.update { it.copy(message = "请输入控制器预授权密钥") }
            return
        }

        val controlUrl = SettingsStore.normalizeServerUrl(snapshot.controlUrl)
        val key = snapshot.preAuthKey.trim()
        settings.setControlUrl(controlUrl)
        secure.savePreAuthKey(key)

        _state.update { it.copy(networkBusy = true, message = null) }
        viewModelScope.launch {
            val result = tailscale.start(
                preAuthKey = key,
                controlUrl = controlUrl,
                hostname = TailscaleManager.suggestedHostname(snapshot.controllerId),
            )
            _state.update {
                it.copy(
                    networkBusy = false,
                    tailnetIp = tailscale.tailnetIp(),
                    message = result.fold(
                        onSuccess = { "网络层已启动" },
                        onFailure = { error -> error.message ?: "网络层启动失败" },
                    ),
                )
            }
        }
    }

    fun stopNetwork() {
        tailscale.stop()
        _state.update {
            it.copy(
                tailnetIp = tailscale.tailnetIp(),
                message = "已请求停止网络层",
            )
        }
    }

    fun clearLocalPairings() {
        secure.clearDevices()
        _state.update { it.copy(message = "已清除本机配对数据") }
    }

    fun consumeMessage() {
        _state.update { it.copy(message = null) }
    }

    companion object {
        fun statusLabel(state: TailscaleState): String = when (state.status) {
            TailscaleStatus.Unavailable -> "未集成"
            TailscaleStatus.Stopped -> "已停止"
            TailscaleStatus.Starting -> "正在启动"
            TailscaleStatus.Running -> "运行中"
            TailscaleStatus.Error -> "错误"
        }
    }
}
