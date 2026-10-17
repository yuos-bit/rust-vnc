package com.rustvnc.client.net

import com.rustvnc.client.wire.ServerHello

/**
 * Connection lifecycle exposed by [ControlClient] and rendered by the
 * session screen. None of these states contain the pairing token.
 */
sealed interface SessionStatus {
    data object Idle : SessionStatus

    data class Connecting(val host: String, val port: Int) : SessionStatus

    data class Connected(
        val hello: ServerHello,
        val host: String,
        val port: Int,
    ) : SessionStatus

    data class Disconnected(val reason: String) : SessionStatus

    data class Failed(val message: String) : SessionStatus
}
