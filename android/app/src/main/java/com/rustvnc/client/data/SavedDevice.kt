package com.rustvnc.client.data

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * A controller-side pairing record. The token is long-lived and is stored
 * only inside EncryptedSharedPreferences.
 */
@Serializable
data class SavedDevice(
    @SerialName("device_code") val deviceCode: String,
    @SerialName("device_name") val deviceName: String,
    val token: String,
    @SerialName("tailscale_ip") val tailscaleIp: String? = null,
    val port: Int = DeviceCode.DEFAULT_PORT,
    val online: Boolean = false,
    @SerialName("paired_at") val pairedAt: Long = System.currentTimeMillis(),
    @SerialName("last_connected_at") val lastConnectedAt: Long? = null,
)
