package com.rustvnc.client.data

import android.content.Context
import android.content.SharedPreferences
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.util.UUID

/**
 * Encrypted storage for pairing tokens, the controller identity and the
 * optional controller pre-auth key.
 *
 * Nothing here is ever logged. The file is backed by a hardware-backed
 * MasterKey where the device supports it.
 */
class SecureStore(context: Context) {

    private val json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
    }

    @Suppress("DEPRECATION")
    private val prefs: SharedPreferences = run {
        val masterKey = MasterKey.Builder(context.applicationContext)
            .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
            .build()
        EncryptedSharedPreferences.create(
            context.applicationContext,
            FILE_NAME,
            masterKey,
            EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
            EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
        )
    }

    private val _devices = MutableStateFlow(loadDevices())
    val devices: StateFlow<List<SavedDevice>> = _devices.asStateFlow()

    /**
     * Stable identifier for this installation. Tokens are bound to it by the
     * pairing server, so it must not change between sessions.
     */
    val controllerId: String = prefs.getString(KEY_CONTROLLER_ID, null)
        ?: ("android-" + UUID.randomUUID().toString()).also { generated ->
            prefs.edit().putString(KEY_CONTROLLER_ID, generated).apply()
        }

    fun get(deviceCode: String): SavedDevice? =
        _devices.value.firstOrNull { it.deviceCode == deviceCode }

    @Synchronized
    fun upsert(device: SavedDevice) {
        val current = _devices.value.toMutableList()
        val index = current.indexOfFirst { it.deviceCode == device.deviceCode }
        if (index >= 0) {
            current[index] = device
        } else {
            current.add(0, device)
        }
        persist(current)
    }

    @Synchronized
    fun markOffline(deviceCode: String) {
        val current = _devices.value
        val index = current.indexOfFirst { it.deviceCode == deviceCode }
        if (index < 0) return
        val updated = current.toMutableList()
        updated[index] = updated[index].copy(online = false)
        persist(updated)
    }

    @Synchronized
    fun remove(deviceCode: String) {
        persist(_devices.value.filterNot { it.deviceCode == deviceCode })
    }

    @Synchronized
    fun clearDevices() {
        persist(emptyList())
    }

    fun preAuthKey(): String? = prefs.getString(KEY_PRE_AUTH, null)

    fun savePreAuthKey(key: String) {
        prefs.edit().putString(KEY_PRE_AUTH, key).apply()
    }

    fun clearPreAuthKey() {
        prefs.edit().remove(KEY_PRE_AUTH).apply()
    }

    private fun loadDevices(): List<SavedDevice> {
        val raw = prefs.getString(KEY_DEVICES, null) ?: return emptyList()
        return try {
            json.decodeFromString<List<SavedDevice>>(raw)
        } catch (_: Exception) {
            // Corrupt or from an incompatible older schema: fail closed and
            // require re-pairing rather than exposing partial records.
            emptyList()
        }
    }

    private fun persist(list: List<SavedDevice>) {
        _devices.value = list
        prefs.edit().putString(KEY_DEVICES, json.encodeToString(list)).apply()
    }

    private companion object {
        const val FILE_NAME = "rvnc_secure"
        const val KEY_DEVICES = "devices"
        const val KEY_CONTROLLER_ID = "controller_id"
        const val KEY_PRE_AUTH = "tailscale_preauth_key"
    }
}
