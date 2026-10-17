package com.rustvnc.client.net

import android.content.Intent
import android.net.VpnService
import android.util.Log

/**
 * Declared VpnService entry point required for an embedded Tailscale stack.
 *
 * IMPORTANT: this scaffold does not establish a tunnel. A production build
 * integrates the official Tailscale Android core (a locally built
 * `libtailscale.aar`, or the `IPNService`/`VpnService` code from
 * tailscale-android) and either:
 *
 *   1. subclasses/merges that service here, or
 *   2. builds a Tun interface with [VpnService.Builder] and hands the file
 *      descriptor to the Go/mobile networking stack, then keeps the
 *      TailscaleController state in sync.
 *
 * Until that is wired, [com.rustvnc.client.net.TailscaleManager] reports
 * "unavailable" and 100.x.y.z addresses are not reachable. See
 * android/README.md.
 */
class RustVncVpnService : VpnService() {

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        Log.i(TAG, "VpnService started; this scaffold does not create a tunnel")
        return START_NOT_STICKY
    }

    override fun onRevoke() {
        Log.i(TAG, "VPN permission revoked by the system")
        stopSelf()
    }

    private companion object {
        const val TAG = "RustVncVpnService"
    }
}
