package com.rustvnc.client.net

import android.content.Context
import android.content.Intent
import android.net.VpnService
import com.rustvnc.tailscale.NoopTailscaleController
import com.rustvnc.tailscale.TailscaleController
import com.rustvnc.tailscale.TailscaleState
import kotlinx.coroutines.flow.StateFlow

/**
 * Thin Android-side wrapper around the embedded Tailscale controller.
 *
 * The real tunnel implementation is supplied by the `tailscale-sdk` module
 * once a locally built AAR is integrated. This scaffold wires the no-op
 * controller, so [isIntegrated] is false and the UI states that clearly
 * instead of pretending a VPN is running.
 */
class TailscaleManager(
    context: Context,
    private val controller: TailscaleController,
) {
    private val appContext: Context = context.applicationContext

    val state: StateFlow<TailscaleState> = controller.state

    /**
     * Whether a real (non-placeholder) controller was supplied. A production
     * build replaces [NoopTailscaleController] in AppContainer.
     */
    val isIntegrated: Boolean
        get() = controller !is NoopTailscaleController

    fun isRunning(): Boolean = controller.isRunning()

    fun tailnetIp(): String? = controller.tailnetIp()

    suspend fun start(
        preAuthKey: String,
        controlUrl: String,
        hostname: String,
    ): Result<Unit> {
        if (preAuthKey.isBlank()) {
            return Result.failure(IllegalArgumentException("预授权密钥不能为空"))
        }
        if (!(controlUrl.startsWith("https://") || controlUrl.startsWith("http://"))) {
            return Result.failure(
                IllegalArgumentException("控制面地址必须以 http:// 或 https:// 开头"),
            )
        }
        return controller.start(preAuthKey.trim(), controlUrl.trim().trimEnd('/'), hostname)
    }

    fun stop() = controller.stop()

    /**
     * Returns the system consent intent, or null when the user has already
     * granted VPN permission. A real controller must call
     * `VpnService.prepare` from an Activity before [start].
     */
    fun prepareIntent(): Intent? = VpnService.prepare(appContext)

    companion object {
        /** Deterministic hostname derived from the installation id. */
        fun suggestedHostname(controllerId: String): String {
            val suffix = controllerId
                .filter { it.isLetterOrDigit() }
                .takeLast(8)
                .lowercase()
                .ifBlank { "controller" }
            return "rvnc-android-$suffix"
        }
    }
}
