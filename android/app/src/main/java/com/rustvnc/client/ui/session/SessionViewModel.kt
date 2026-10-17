package com.rustvnc.client.ui.session

import android.content.Context
import android.graphics.Bitmap
import android.os.SystemClock
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.rustvnc.client.data.PairingRepository
import com.rustvnc.client.data.SavedDevice
import com.rustvnc.client.net.ControlClient
import com.rustvnc.client.net.SessionForegroundService
import com.rustvnc.client.net.SessionStatus
import com.rustvnc.client.net.TailscaleManager
import com.rustvnc.client.wire.InputEvent
import com.rustvnc.client.wire.MouseWireButton
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * Owns the lifetime of one control session and translates UI gestures into
 * wire input events.
 */
class SessionViewModel(
    private val context: Context,
    private val repository: PairingRepository,
    private val tailscale: TailscaleManager,
    private val deviceCode: String,
) : ViewModel() {

    val device: SavedDevice? = repository.device(deviceCode)
    private val client: ControlClient? = device?.let {
        ControlClient(
            deviceCode = it.deviceCode,
            token = it.token,
            controllerId = repository.controllerId,
            controllerName = repository.controllerNameFlow.value,
        )
    }

    private val _status = MutableStateFlow<SessionStatus>(SessionStatus.Idle)
    val status: StateFlow<SessionStatus> = _status.asStateFlow()

    private val _frame = MutableStateFlow<Bitmap?>(null)
    val frame: StateFlow<Bitmap?> = _frame.asStateFlow()

    private val _resolving = MutableStateFlow(false)
    val resolving: StateFlow<Boolean> = _resolving.asStateFlow()

    private val _warning = MutableStateFlow<String?>(null)
    val warning: StateFlow<String?> = _warning.asStateFlow()

    private var connectingJob: Job? = null
    private var serviceStarted = false
    private var lastMoveAt = 0L

    init {
        val active = client
        if (active == null) {
            _status.value = SessionStatus.Failed("本机没有该设备的配对记录，请返回设备列表重新配对。")
        } else {
            viewModelScope.launch {
                active.status.collect { value ->
                    _status.value = value
                    when (value) {
                        is SessionStatus.Connected -> ensureForegroundService()
                        is SessionStatus.Failed, is SessionStatus.Disconnected -> stopForegroundService()
                        else -> Unit
                    }
                }
            }
            viewModelScope.launch {
                active.frame.collect { bitmap -> _frame.value = bitmap }
            }
            connect()
        }
    }

    fun connect() {
        val saved = device
        val active = client
        if (saved == null || active == null) {
            _status.value = SessionStatus.Failed(
                "本机没有该设备的配对记录，请返回设备列表重新配对。",
            )
            return
        }
        if (_resolving.value) return

        _resolving.value = true
        _warning.value = when {
            !tailscale.isIntegrated ->
                "Tailscale 网络层未集成（Noop 控制器）；只有可直接路由的地址才能连接。"
            !tailscale.isRunning() ->
                "Tailscale 网络层未运行，请先在设置中启动网络。"
            else -> null
        }

        connectingJob?.cancel()
        connectingJob = viewModelScope.launch {
            try {
                val target = repository.connect(saved)
                _status.value = SessionStatus.Connecting(target.tailscaleIp, target.port)
                active.connect(target.tailscaleIp, target.port)
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Exception) {
                _status.value = SessionStatus.Failed(
                    error.message?.takeIf(String::isNotBlank) ?: "无法解析被控端地址",
                )
                stopForegroundService()
            } finally {
                _resolving.value = false
            }
        }
    }

    fun retry() {
        connectingJob?.cancel()
        _resolving.value = false
        stopForegroundService()
        client?.disconnect()
        _status.value = SessionStatus.Idle
        _frame.value = null
        connect()
    }

    fun disconnect() {
        // Cancel an in-flight address resolution; otherwise it would start a
        // session after the user already cancelled.
        connectingJob?.cancel()
        _resolving.value = false
        client?.disconnect()
        stopForegroundService()
    }

    /** Throttled cursor movement; the queue itself also bounds latency. */
    fun move(x: Double, y: Double) {
        val now = SystemClock.elapsedRealtime()
        if (now - lastMoveAt < 16L) return
        lastMoveAt = now
        client?.sendInput(
            InputEvent.MouseMove(x.coerceIn(0.0, 1.0), y.coerceIn(0.0, 1.0)),
        )
    }

    fun click(x: Double, y: Double, button: MouseWireButton) {
        val clampedX = x.coerceIn(0.0, 1.0)
        val clampedY = y.coerceIn(0.0, 1.0)
        client?.sendInput(InputEvent.MouseMove(clampedX, clampedY))
        client?.sendInput(InputEvent.MouseButton(button, pressed = true))
        client?.sendInput(InputEvent.MouseButton(button, pressed = false))
    }

    /** Deltas are in wheel notches; the agent multiplies by 120. */
    fun wheel(deltaX: Double, deltaY: Double) {
        if (deltaX == 0.0 && deltaY == 0.0) return
        client?.sendInput(InputEvent.Wheel(deltaX, deltaY))
    }

    fun sendText(text: String) {
        if (text.isEmpty()) return
        client?.sendInput(InputEvent.Text(text))
    }

    fun sendKey(code: String) {
        client?.sendInput(InputEvent.Key(code, pressed = true))
        client?.sendInput(InputEvent.Key(code, pressed = false))
    }

    override fun onCleared() {
        client?.close()
        stopForegroundService()
        super.onCleared()
    }

    private fun ensureForegroundService() {
        if (serviceStarted) return
        serviceStarted = true
        runCatching {
            SessionForegroundService.start(
                context,
                device?.deviceName ?: "远程设备",
            )
        }
    }

    private fun stopForegroundService() {
        if (!serviceStarted) return
        serviceStarted = false
        runCatching { SessionForegroundService.stop(context) }
    }
}
