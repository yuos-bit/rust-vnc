package com.rustvnc.client.net

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import com.rustvnc.client.wire.ClientCodec
import com.rustvnc.client.wire.ClientMessage
import com.rustvnc.client.wire.FrameCodec
import com.rustvnc.client.wire.FrameFormat
import com.rustvnc.client.wire.InputEvent
import com.rustvnc.client.wire.ServerCodec
import com.rustvnc.client.wire.ServerMessage
import com.rustvnc.client.wire.VideoFrame
import com.rustvnc.client.wire.Wire
import com.rustvnc.client.wire.WireFormatException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.net.ConnectException
import java.net.InetSocketAddress
import java.net.NoRouteToHostException
import java.net.Socket
import java.net.SocketTimeoutException
import java.net.UnknownHostException
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

/**
 * Single TCP control session speaking the RustVNC wire protocol.
 *
 * The socket is opened and read on [Dispatchers.IO]. Video frames are decoded
 * on a dedicated coroutine; a conflated channel drops stale frames instead of
 * queueing them, so latency stays bounded when the UI cannot keep up.
 */
class ControlClient(
    private val deviceCode: String,
    private val token: String,
    private val controllerId: String,
    private val controllerName: String,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val generation = AtomicInteger(0)

    /** Guards generation changes together with the shared [socket] field. */
    private val stateLock = Any()

    private val _status = MutableStateFlow<SessionStatus>(SessionStatus.Idle)
    val status: StateFlow<SessionStatus> = _status.asStateFlow()

    private val _frame = MutableStateFlow<Bitmap?>(null)
    val frame: StateFlow<Bitmap?> = _frame.asStateFlow()

    @Volatile
    private var socket: Socket? = null

    @Volatile
    private var outgoing: Channel<ClientMessage>? = null

    @Volatile
    private var userClosed: Boolean = false

    private var sessionJob: Job? = null

    /**
     * Starts a new session. Any previous session is cancelled; status updates
     * from the old one are ignored thanks to the generation counter.
     */
    fun connect(host: String, port: Int) {
        val gen: Int
        synchronized(stateLock) {
            gen = generation.incrementAndGet()
            userClosed = false
            // Unblock a previous read loop (a blocking socket read does not
            // observe coroutine cancellation) before starting the new session.
            closeSocket()
            sessionJob?.cancel()
        }
        sessionJob = scope.launch { runSession(gen, host, port) }
    }

    fun sendInput(event: InputEvent) {
        outgoing?.trySend(ClientMessage.Input(event))
    }

    fun sendPing(id: Long) {
        outgoing?.trySend(ClientMessage.Ping(id))
    }

    /**
     * Sends Bye and closes the socket shortly after so the frame has a chance
     * to flush. The read loop then reports the disconnect.
     */
    fun disconnect() {
        userClosed = true
        val channel = outgoing
        channel?.trySend(ClientMessage.Bye)
        // Capture the current socket: a fast reconnect or retry may replace
        // the field before the delayed close runs.
        val target = socket
        scope.launch {
            delay(120)
            target?.let { closeSocket(it) }
        }
        if (sessionJob?.isActive != true) {
            _status.value = SessionStatus.Disconnected("已断开连接")
        }
    }

    /** Tears everything down; the instance is not reusable afterwards. */
    fun close() {
        synchronized(stateLock) {
            userClosed = true
            generation.incrementAndGet()
            sessionJob?.cancel()
            closeSocket()
        }
        scope.cancel()
        _frame.value = null
    }

    private suspend fun runSession(gen: Int, host: String, port: Int) {
        synchronized(stateLock) {
            if (gen != generation.get()) return
            _status.value = SessionStatus.Connecting(host, port)
            _frame.value = null
        }

        val failure = AtomicReference<String?>(null)
        var activeSocket: Socket? = null
        var outChannel: Channel<ClientMessage>? = null
        var established = false

        try {
            val opened = Socket()
            activeSocket = opened
            opened.tcpNoDelay = true
            opened.connect(InetSocketAddress(host, port), CONNECT_TIMEOUT_MS)
            opened.soTimeout = HANDSHAKE_TIMEOUT_MS

            val input = BufferedInputStream(opened.getInputStream())
            val output = BufferedOutputStream(opened.getOutputStream())
            val channel = Channel<ClientMessage>(capacity = OUTGOING_CAPACITY)
            outChannel = channel

            val current = synchronized(stateLock) {
                if (gen != generation.get()) {
                    false
                } else {
                    socket = opened
                    outgoing = channel
                    true
                }
            }
            if (!current) throw SessionEnded("会话已被新的连接替换")

            FrameCodec.write(
                output,
                ClientCodec.encode(
                    ClientMessage.Hello(
                        deviceCode = deviceCode,
                        token = token,
                        controllerId = controllerId,
                        controllerName = controllerName,
                        platform = "android",
                    ),
                ),
            )

            val firstBody = FrameCodec.read(input)
                ?: throw SessionEnded("被控端关闭了连接")

            when (val message = ServerCodec.decode(firstBody)) {
                is ServerMessage.Hello -> {
                    if (!message.hello.accepted) {
                        throw SessionEnded("被控端拒绝建立会话")
                    }
                    if (message.hello.wireVersion != Wire.WIRE_VERSION) {
                        throw SessionEnded(
                            "协议版本不兼容：服务器 v${message.hello.wireVersion}，客户端 v${Wire.WIRE_VERSION}",
                        )
                    }
                    opened.soTimeout = READ_TIMEOUT_MS
                    established = true
                    if (gen == generation.get()) {
                        _status.value = SessionStatus.Connected(message.hello, host, port)
                    }
                }

                is ServerMessage.Error -> throw SessionEnded(
                    friendlyServerError(message.body.code, message.body.message),
                )

                else -> throw SessionEnded("握手阶段收到意外消息")
            }

            if (gen != generation.get()) throw SessionEnded("会话已被新的连接替换")
            val videoFrames = Channel<VideoFrame>(Channel.CONFLATED)
            val writerJob = scope.launch {
                writerLoop(output, channel, failure, opened)
            }
            val pingJob = scope.launch { pingLoop(channel) }
            val decoderJob = scope.launch { decoderLoop(videoFrames, gen) }

            try {
                val reason = readLoop(input, videoFrames, gen)
                throw SessionEnded(failure.get() ?: reason)
            } finally {
                writerJob.cancel()
                pingJob.cancel()
                decoderJob.cancel()
                channel.close()
                videoFrames.close()
            }
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (ended: SessionEnded) {
            if (gen == generation.get()) {
                _status.value = when {
                    userClosed -> SessionStatus.Disconnected("已断开连接")
                    established -> SessionStatus.Disconnected(ended.message ?: "连接已结束")
                    else -> SessionStatus.Failed(ended.message ?: "连接已结束")
                }
            }
        } catch (error: Exception) {
            if (gen == generation.get()) {
                val message = failure.get() ?: friendly(error)
                _status.value = when {
                    userClosed -> SessionStatus.Disconnected("已断开连接")
                    established -> SessionStatus.Disconnected(message)
                    else -> SessionStatus.Failed(message)
                }
            }
        } finally {
            val channel = outChannel
            if (channel != null) {
                synchronized(stateLock) {
                    if (outgoing === channel) outgoing = null
                }
            }
            val closed = activeSocket
            if (closed != null) closeSocket(closed)
            if (gen == generation.get()) _frame.value = null
        }
    }

    private suspend fun writerLoop(
        output: OutputStream,
        channel: Channel<ClientMessage>,
        failure: AtomicReference<String?>,
        activeSocket: Socket,
    ) {
        try {
            for (message in channel) {
                FrameCodec.write(output, ClientCodec.encode(message))
            }
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Exception) {
            failure.compareAndSet(null, "发送数据失败：${error.message ?: "连接中断"}")
            closeSocket(activeSocket)
        }
    }

    private suspend fun pingLoop(channel: Channel<ClientMessage>) {
        var id = 0L
        while (true) {
            delay(PING_INTERVAL_MS)
            id += 1
            if (channel.trySend(ClientMessage.Ping(id)).isFailure) return
        }
    }

    private fun readLoop(
        input: InputStream,
        frames: Channel<VideoFrame>,
        gen: Int,
    ): String {
        while (true) {
            if (gen != generation.get()) return "会话已被新的连接替换"
            val body = try {
                FrameCodec.read(input) ?: return "被控端已断开连接"
            } catch (_: SocketTimeoutException) {
                return "连接超时，会话已结束"
            } catch (error: IOException) {
                return "读取数据失败：${error.message ?: "网络中断"}"
            }

            when (val message = ServerCodec.decode(body)) {
                is ServerMessage.Video -> {
                    if (message.frame.format == FrameFormat.Jpeg) {
                        frames.trySend(message.frame)
                    }
                }

                is ServerMessage.Pong -> Unit
                is ServerMessage.Error -> throw SessionEnded(
                    friendlyServerError(message.body.code, message.body.message),
                )

                // Duplicate hellos are ignored, matching the Rust agent.
                is ServerMessage.Hello -> Unit
            }
        }
    }

    private suspend fun decoderLoop(frames: Channel<VideoFrame>, gen: Int) {
        for (video in frames) {
            val bitmap = runCatching { decodeJpeg(video) }.getOrNull() ?: continue
            if (gen == generation.get()) _frame.value = bitmap
        }
    }

    private fun decodeJpeg(video: VideoFrame): Bitmap? {
        if (video.data.isEmpty()) return null

        var sample = 1
        val width = video.width
        val height = video.height
        if (width in 1L..100_000L && height in 1L..100_000L) {
            while (sample < 64 && (width / sample) * (height / sample) > MAX_DECODED_PIXELS) {
                sample *= 2
            }
        }

        val options = BitmapFactory.Options().apply {
            inSampleSize = sample
            inPreferredConfig = Bitmap.Config.ARGB_8888
        }
        return BitmapFactory.decodeByteArray(video.data, 0, video.data.size, options)
    }

    private fun closeSocket(target: Socket? = socket) {
        try {
            target?.close()
        } catch (_: IOException) {
            // Already closed.
        }
        synchronized(stateLock) {
            if (target === socket) socket = null
        }
    }

    private fun friendly(error: Exception): String = when (error) {
        is SocketTimeoutException -> "连接超时，请检查 Tailscale 网络"
        is ConnectException -> "无法连接被控端（连接被拒绝或网络不可达）"
        is NoRouteToHostException -> "无法路由到被控端，请确认 Tailscale 网络已建立"
        is UnknownHostException -> "无法解析被控端地址"
        is WireFormatException -> "协议错误：${error.message ?: "数据格式不兼容"}"
        else -> "连接失败：${error.message ?: error.javaClass.simpleName}"
    }

    private fun friendlyServerError(code: String, message: String): String = when (code) {
        "unauthorized" -> "配对令牌无效、已被吊销或已过期，请重新配对。"
        "version_mismatch" -> "控制协议版本不兼容，请升级客户端或被控端。"
        "bad_magic", "bad_handshake" -> "目标不是有效的 RustVNC 被控端。"
        "wrong_device" -> "设备码与被控端不匹配。"
        "capture_failed" -> "被控端屏幕采集失败：$message"
        "auth_unavailable" -> "被控端暂时无法校验令牌，请稍后重试。"
        else -> "$code: $message"
    }

    private class SessionEnded(message: String) : Exception(message)

    private companion object {
        const val CONNECT_TIMEOUT_MS = 12_000
        const val HANDSHAKE_TIMEOUT_MS = 12_000
        const val READ_TIMEOUT_MS = 30_000
        const val PING_INTERVAL_MS = 5_000L
        const val OUTGOING_CAPACITY = 256
        const val MAX_DECODED_PIXELS = 2_000_000L
    }
}
