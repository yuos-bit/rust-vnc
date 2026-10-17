package com.rustvnc.client.wire

/**
 * Decodes messages sent by the controlled endpoint. The body has already had
 * its `u32_le` length prefix removed by [FrameCodec].
 */
object ServerCodec {

    fun decode(body: ByteArray): ServerMessage {
        val reader = BincodeReader(body)
        val message = when (val variant = reader.enumIndex()) {
            VARIANT_HELLO -> ServerMessage.Hello(decodeHello(reader))
            VARIANT_VIDEO -> ServerMessage.Video(decodeVideo(reader))
            VARIANT_PONG -> ServerMessage.Pong(reader.u64())
            VARIANT_ERROR -> ServerMessage.Error(
                ServerError(
                    code = reader.string(MAX_ERROR_BYTES),
                    message = reader.string(MAX_ERROR_BYTES),
                ),
            )

            else -> throw WireFormatException("未知的服务器消息类型：$variant")
        }
        reader.requireFullyConsumed()
        return message
    }

    private fun decodeHello(reader: BincodeReader): ServerHello = ServerHello(
        wireVersion = reader.u16(),
        accepted = reader.bool(),
        sessionId = reader.string(MAX_SESSION_ID_BYTES),
        deviceName = reader.string(MAX_DEVICE_NAME_BYTES),
        width = reader.u32(),
        height = reader.u32(),
        fps = reader.u16(),
    )

    private fun decodeVideo(reader: BincodeReader): VideoFrame {
        val seq = reader.u64()
        val width = reader.u32()
        val height = reader.u32()
        val timestampMs = reader.u64()
        val formatIndex = reader.enumIndex()
        val format = FrameFormat.entries.firstOrNull { it.wireIndex == formatIndex }
            ?: throw WireFormatException("不支持的画面格式：$formatIndex")
        val data = reader.bytes(Wire.MAX_FRAME_BYTES)
        if (data.isEmpty()) throw WireFormatException("画面数据为空")
        return VideoFrame(
            seq = seq,
            width = width,
            height = height,
            timestampMs = timestampMs,
            format = format,
            data = data,
        )
    }

    // ServerMessage variant indices.
    private const val VARIANT_HELLO = 0
    private const val VARIANT_VIDEO = 1
    private const val VARIANT_PONG = 2
    private const val VARIANT_ERROR = 3

    private const val MAX_SESSION_ID_BYTES = 256
    private const val MAX_DEVICE_NAME_BYTES = 4096
    private const val MAX_ERROR_BYTES = 4096
}
