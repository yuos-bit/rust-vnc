package com.rustvnc.client.data

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.withContext

data class PairOutcome(
    val device: SavedDevice,
    /** Non-fatal issue after a successful pairing, e.g. the endpoint is offline. */
    val warning: String? = null,
)

/**
 * High-level pairing flow:
 *
 * 1. POST /api/v1/pair with the device code + password.
 * 2. Persist the long-lived token in [SecureStore].
 * 3. POST /api/v1/connect to resolve the current tailnet address/port.
 * 4. Persist the resolved address so the device list can show it.
 *
 * A successful pair whose connect step fails (offline endpoint) is still
 * persisted: the token is valid and a later connect can succeed.
 */
class PairingRepository(
    private val api: ApiClient,
    private val settings: SettingsStore,
    private val secure: SecureStore,
) {
    val devices: StateFlow<List<SavedDevice>> = secure.devices
    val serverUrlFlow: StateFlow<String> = settings.serverUrl
    val controllerNameFlow: StateFlow<String> = settings.controllerName
    val controllerId: String = secure.controllerId

    fun serverUrl(): String = settings.serverUrl.value

    fun device(deviceCode: String): SavedDevice? = secure.get(deviceCode)

    suspend fun pair(rawCode: String, password: String): PairOutcome = withContext(Dispatchers.IO) {
        val server = requireServer()
        val code = DeviceCode.sanitize(rawCode)
        if (!DeviceCode.isValid(code)) {
            throw ApiException("invalid_code", "请输入完整的 6 位设备码")
        }
        if (password.isEmpty()) {
            throw ApiException("empty_password", "请输入配对密码")
        }

        val response = api.pair(
            server,
            PairRequestDto(
                deviceCode = code,
                password = password,
                controllerId = controllerId,
                controllerName = settings.controllerName.value,
                platform = "android",
            ),
        )

        var device = SavedDevice(
            deviceCode = response.deviceCode,
            deviceName = response.deviceName,
            token = response.token,
            tailscaleIp = response.tailscaleIp,
            port = DeviceCode.DEFAULT_PORT,
            online = response.online,
            pairedAt = System.currentTimeMillis(),
        )
        secure.upsert(device)

        val warning = try {
            val target = api.connect(
                server,
                ConnectRequestDto(
                    deviceCode = device.deviceCode,
                    token = device.token,
                    controllerId = controllerId,
                ),
            )
            device = device.copy(
                deviceName = target.deviceName,
                tailscaleIp = target.tailscaleIp,
                port = target.port,
                online = target.online,
                lastConnectedAt = System.currentTimeMillis(),
            )
            secure.upsert(device)
            null
        } catch (fatal: Exception) {
            if (fatal is kotlinx.coroutines.CancellationException) throw fatal
            val apiError = fatal as? ApiException
            secure.markOffline(device.deviceCode)
            if (apiError?.code == "device_offline") {
                "配对成功，但被控端当前离线；设备上线后即可连接。"
            } else {
                "配对成功，但暂时无法解析被控端地址：${fatal.message ?: "未知错误"}"
            }
        }

        PairOutcome(device = secure.get(device.deviceCode) ?: device, warning = warning)
    }

    /**
     * Resolves the current tailnet address. A 409 `device_offline` or a 401
     * invalid/revoked token marks the local record offline so the list does
     * not keep showing a stale green dot.
     */
    suspend fun connect(device: SavedDevice): ConnectResponseDto = withContext(Dispatchers.IO) {
        val server = requireServer()
        try {
            val target = api.connect(
                server,
                ConnectRequestDto(
                    deviceCode = device.deviceCode,
                    token = device.token,
                    controllerId = controllerId,
                ),
            )
            secure.upsert(
                device.copy(
                    deviceName = target.deviceName,
                    tailscaleIp = target.tailscaleIp,
                    port = target.port,
                    online = target.online,
                    lastConnectedAt = System.currentTimeMillis(),
                ),
            )
            target
        } catch (fatal: Exception) {
            if (fatal is kotlinx.coroutines.CancellationException) throw fatal
            val apiError = fatal as? ApiException
            if (apiError != null && (apiError.code == "device_offline" || apiError.httpStatus == 401)) {
                secure.markOffline(device.deviceCode)
            }
            throw fatal
        }
    }

    fun removeLocal(deviceCode: String) {
        secure.remove(deviceCode)
    }

    private fun requireServer(): String {
        val server = settings.serverUrl.value
        if (server.isBlank()) {
            throw ApiException("config", "请先在设置中填写配对服务器地址")
        }
        if (!SettingsStore.isValidServerUrl(server)) {
            throw ApiException("config", "配对服务器地址必须以 http:// 或 https:// 开头")
        }
        return SettingsStore.normalizeServerUrl(server)
    }
}
