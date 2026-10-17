package com.rustvnc.tailscale

import kotlinx.coroutines.flow.StateFlow

/**
 * Lifecycle states reported by a Tailscale network controller.
 */
enum class TailscaleStatus {
    /** No implementation is bundled in this build. */
    Unavailable,

    /** A controller exists but the tunnel is not up. */
    Stopped,

    /** A tunnel is being established (login / WireGuard handshake). */
    Starting,

    /** The tunnel is up and a 100.x.y.z address is assigned. */
    Running,

    /** The tunnel failed or was revoked. */
    Error,
}

/**
 * Snapshot of the network layer. [message] is user-facing Chinese text and
 * must never contain secrets (pre-auth keys, tokens).
 */
data class TailscaleState(
    val status: TailscaleStatus = TailscaleStatus.Stopped,
    val message: String? = null,
)

/**
 * Integration point for the embedded Tailscale network stack.
 *
 * The official Tailscale Android SDK is not published to Maven. A real
 * implementation must be built locally (gomobile `libtailscale.aar`, or the
 * VpnService plumbing from tailscale-android) and bound to this interface.
 * The scaffold ships [NoopTailscaleController] so the rest of the app can be
 * developed and tested without pretending that the tunnel exists.
 */
interface TailscaleController {
    /** Observable state, collected by Settings and the session screen. */
    val state: StateFlow<TailscaleState>

    /**
     * Starts the tunnel.
     *
     * @param preAuthKey a pre-auth key provisioned for this controller. The
     *   pairing server can mint one through `POST /api/v1/controllers/enroll`
     *   (or an administrator can create it with `headscale preauthkeys create`).
     * @param controlUrl Headscale/Tailscale control-plane URL.
     * @param hostname hostname the controller registers under in the tailnet.
     */
    suspend fun start(
        preAuthKey: String,
        controlUrl: String,
        hostname: String,
    ): Result<Unit>

    /** Stops the tunnel and tears down the VpnService interface. */
    fun stop()

    /** The assigned tailnet IPv4 address, or null when the tunnel is down. */
    fun tailnetIp(): String?

    /** Whether the tunnel is currently running. */
    fun isRunning(): Boolean
}
