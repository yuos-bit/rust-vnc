package com.rustvnc.client.data

import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.serialization.json.Json
import okhttp3.Call
import okhttp3.Callback
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/**
 * Error surfaced by the pairing API. [message] is safe to show to the user
 * and never contains the pairing token or the password.
 */
class ApiException(
    val code: String,
    override val message: String,
    val httpStatus: Int? = null,
    val retryAfterSeconds: Long? = null,
) : Exception(message)

/**
 * Minimal HTTPS client for the pairing backend.
 *
 * Endpoints (see crates/rv-server/src/main.rs):
 *   POST {server}/api/v1/pair
 *   POST {server}/api/v1/connect
 *   POST {server}/api/v1/controllers/enroll
 *
 * No logging interceptor is installed on purpose: request bodies contain the
 * password on pairing and the bearer token on connect.
 */
class ApiClient(
    private val http: OkHttpClient = defaultClient(),
    private val json: Json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
        explicitNulls = false
    },
) {

    suspend fun pair(serverUrl: String, request: PairRequestDto): PairResponseDto {
        val body = json.encodeToString(PairRequestDto.serializer(), request)
        val payload = postJson("$serverUrl/api/v1/pair", body)
        return try {
            json.decodeFromString(PairResponseDto.serializer(), payload)
        } catch (_: Exception) {
            throw ApiException("bad_response", "服务器返回的配对数据无法解析")
        }
    }

    suspend fun connect(serverUrl: String, request: ConnectRequestDto): ConnectResponseDto {
        val body = json.encodeToString(ConnectRequestDto.serializer(), request)
        val payload = postJson("$serverUrl/api/v1/connect", body)
        return try {
            json.decodeFromString(ConnectResponseDto.serializer(), payload)
        } catch (_: Exception) {
            throw ApiException("bad_response", "服务器返回的连接数据无法解析")
        }
    }

    /**
     * Asks the pairing backend to mint a single-use tailnet pre-auth key for
     * this controller. The key is returned once and is not stored server-side.
     */
    suspend fun enrollController(
        serverUrl: String,
        request: EnrollControllerRequestDto,
    ): EnrollControllerResponseDto {
        val body = json.encodeToString(EnrollControllerRequestDto.serializer(), request)
        val payload = postJson("$serverUrl/api/v1/controllers/enroll", body)
        return try {
            json.decodeFromString(EnrollControllerResponseDto.serializer(), payload)
        } catch (_: Exception) {
            throw ApiException("bad_response", "服务器返回的网络密钥数据无法解析")
        }
    }

    private suspend fun postJson(url: String, body: String): String {
        val request = try {
            Request.Builder()
                .url(url)
                .post(body.toRequestBody(JSON_MEDIA_TYPE))
                .header("Accept", "application/json")
                .header("User-Agent", USER_AGENT)
                .build()
        } catch (_: IllegalArgumentException) {
            throw ApiException("config", "配对服务器地址无效")
        }
        return execute(request)
    }

    private suspend fun execute(request: Request): String =
        suspendCancellableCoroutine { continuation ->
            val call = http.newCall(request)
            continuation.invokeOnCancellation { call.cancel() }

            call.enqueue(object : Callback {
                override fun onFailure(call: Call, error: IOException) {
                    if (!continuation.isActive) return
                    continuation.resumeWithException(
                        ApiException(
                            code = "network",
                            message = "无法连接配对服务器：${error.message?.take(120) ?: "网络错误"}",
                        ),
                    )
                }

                override fun onResponse(call: Call, response: Response) {
                    response.use { result ->
                        val payload = result.body?.string().orEmpty()
                        if (result.isSuccessful) {
                            if (continuation.isActive) continuation.resume(payload)
                            return
                        }

                        val retryAfter = result.header("Retry-After")?.toLongOrNull()
                        val apiError = runCatching {
                            json.decodeFromString(ApiErrorEnvelope.serializer(), payload)
                        }.getOrNull()

                        val code = apiError?.error?.code ?: "http_${result.code}"
                        val message = apiError?.error?.message ?: when (result.code) {
                            401 -> "认证失败"
                            403 -> "没有权限执行此操作"
                            429 -> "尝试过于频繁，请稍后再试"
                            else -> "服务器返回 HTTP ${result.code}"
                        }
                        if (continuation.isActive) {
                            continuation.resumeWithException(
                                ApiException(
                                    code = code,
                                    message = message,
                                    httpStatus = result.code,
                                    retryAfterSeconds = retryAfter,
                                ),
                            )
                        }
                    }
                }
            })
        }

    companion object {
        private const val USER_AGENT = "RustVNC-Android/0.1.0"
        private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()

        fun defaultClient(): OkHttpClient = OkHttpClient.Builder()
            .connectTimeout(8, TimeUnit.SECONDS)
            .readTimeout(20, TimeUnit.SECONDS)
            .writeTimeout(20, TimeUnit.SECONDS)
            .callTimeout(25, TimeUnit.SECONDS)
            .retryOnConnectionFailure(true)
            .build()
    }
}
