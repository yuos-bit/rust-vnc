package com.rustvnc.client.data

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/*
 * JSON DTOs mirroring crates/rv-proto/src/api.rs. The server uses serde's
 * default snake_case field names, hence the explicit @SerialName annotations.
 */

@Serializable
data class PairRequestDto(
    @SerialName("device_code") val deviceCode: String,
    val password: String,
    @SerialName("controller_id") val controllerId: String,
    @SerialName("controller_name") val controllerName: String,
    val platform: String = "android",
)

@Serializable
data class PairResponseDto(
    val token: String,
    @SerialName("token_id") val tokenId: String,
    @SerialName("device_code") val deviceCode: String,
    @SerialName("device_name") val deviceName: String,
    @SerialName("tailscale_ip") val tailscaleIp: String? = null,
    val online: Boolean = false,
    @SerialName("server_version") val serverVersion: String = "",
)

@Serializable
data class ConnectRequestDto(
    @SerialName("device_code") val deviceCode: String,
    val token: String,
    @SerialName("controller_id") val controllerId: String,
)

@Serializable
data class ConnectResponseDto(
    @SerialName("device_code") val deviceCode: String,
    @SerialName("device_name") val deviceName: String,
    @SerialName("tailscale_ip") val tailscaleIp: String,
    val port: Int,
    val online: Boolean,
)

@Serializable
data class ApiErrorEnvelope(
    val error: ErrorBodyDto,
)

@Serializable
data class EnrollControllerRequestDto(
    val name: String,
    val platform: String = "android",
    val fingerprint: String,
    @SerialName("controller_id") val controllerId: String,
)

@Serializable
data class EnrollControllerResponseDto(
    @SerialName("preauth_key") val preauthKey: String,
    @SerialName("control_url") val controlUrl: String,
    val hostname: String,
    @SerialName("derp_url") val derpUrl: String? = null,
)

@Serializable
data class ErrorBodyDto(
    val code: String,
    val message: String,
)
