package com.rustvnc.client.wire

import java.io.InputStream
import java.io.OutputStream

/**
 * `u32 little-endian length || bincode body` framing, exactly as implemented
 * by `rv-engine::codec`. The length is validated before any allocation.
 */
object FrameCodec {

    fun write(output: OutputStream, payload: ByteArray) {
        if (payload.isEmpty()) {
            throw WireFormatException("拒绝发送空帧")
        }
        if (payload.size > Wire.MAX_FRAME_BYTES) {
            throw WireFormatException("发送帧过大：${payload.size} 字节")
        }
        val length = payload.size
        output.write(
            byteArrayOf(
                (length and 0xFF).toByte(),
                ((length ushr 8) and 0xFF).toByte(),
                ((length ushr 16) and 0xFF).toByte(),
                ((length ushr 24) and 0xFF).toByte(),
            ),
        )
        output.write(payload)
        output.flush()
    }

    /**
     * Reads one frame. Returns null on a clean EOF at a message boundary,
     * matching the Rust reader.
     */
    fun read(input: InputStream): ByteArray? {
        val header = ByteArray(4)
        var headerRead = 0
        while (headerRead < 4) {
            val count = input.read(header, headerRead, 4 - headerRead)
            if (count < 0) {
                if (headerRead == 0) return null
                throw WireFormatException("帧长度前缀不完整")
            }
            headerRead += count
        }

        val length =
            (header[0].toLong() and 0xFF) or
                ((header[1].toLong() and 0xFF) shl 8) or
                ((header[2].toLong() and 0xFF) shl 16) or
                ((header[3].toLong() and 0xFF) shl 24)

        if (length == 0L || length > Wire.MAX_FRAME_BYTES.toLong()) {
            throw WireFormatException("非法帧长度：$length 字节")
        }

        val body = ByteArray(length.toInt())
        var bodyRead = 0
        while (bodyRead < body.size) {
            val count = input.read(body, bodyRead, body.size - bodyRead)
            if (count < 0) throw WireFormatException("帧数据不完整")
            bodyRead += count
        }
        return body
    }
}
