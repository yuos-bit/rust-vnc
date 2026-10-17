package com.rustvnc.tailscale

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Placeholder used until a locally built Tailscale AAR is integrated.
 *
 * This deliberately reports "unavailable" instead of faking a tunnel. The app
 * can still pair through the HTTPS API and can connect to devices that are
 * directly routable, but 100.x.y.z tailnet addresses will not be reachable.
 */
class NoopTailscaleController : TailscaleController {
    private val _state = MutableStateFlow(
        TailscaleState(
            status = TailscaleStatus.Unavailable,
            message = "当前构建未集成 Tailscale 网络层（Noop 控制器）",
        ),
    )

    override val state: StateFlow<TailscaleState> = _state.asStateFlow()

    override suspend fun start(
        preAuthKey: String,
        controlUrl: String,
        hostname: String,
    ): Result<Unit> = Result.failure(
        IllegalStateException(
            "当前构建未集成 Tailscale 网络层。请先构建并接入 libtailscale AAR，详见 android/README.md。",
        ),
    )

    override fun stop() = Unit

    override fun tailnetIp(): String? = null

    override fun isRunning(): Boolean = false
}
