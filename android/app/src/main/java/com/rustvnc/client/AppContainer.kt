package com.rustvnc.client

import android.content.Context
import com.rustvnc.client.data.ApiClient
import com.rustvnc.client.data.PairingRepository
import com.rustvnc.client.data.SecureStore
import com.rustvnc.client.data.SettingsStore
import com.rustvnc.client.net.TailscaleManager
import com.rustvnc.tailscale.NoopTailscaleController

/**
 * Manual dependency container shared by the whole process.
 *
 * Replace [NoopTailscaleController] with the real controller once the locally
 * built Tailscale AAR is integrated; nothing else in the app needs to change.
 */
class AppContainer(context: Context) {
    private val appContext = context.applicationContext

    val settings = SettingsStore(appContext)
    val secure = SecureStore(appContext)
    val api = ApiClient()
    val repository = PairingRepository(api, settings, secure)
    val tailscale = TailscaleManager(appContext, NoopTailscaleController())
}
