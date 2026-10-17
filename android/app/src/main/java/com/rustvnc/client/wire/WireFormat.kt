package com.rustvnc.client.wire

import java.io.ByteArrayOutputStream

/**
 * Wire constants shared with `crates/rv-proto/src/wire.rs`.
 */
object Wire {
    const val MAGIC = "RVNC"
    const val WIRE_VERSION = 1
    const val DEFAULT_PORT = 21416

    /** Same cap as the Rust side: 16 MiB. */
    const val MAX_FRAME_BYTES = 16 * 1024 * 1024

    /** Defensive cap for control strings (never close to 16 MiB). */
    const val MAX_STRING_BYTES = 1024 * 1024
}

class WireFormatException(message: String) : Exception(message)

/**
 * Encoder for bincode 2.0.1's `config::standard()` format:
 *
 * - little endian fixed-width values,
 * - unsigned integers use bincode's variable-int encoding:
 *   0..=250 as one byte; 251..=65535 as `0xFB` + u16 LE;
 *   65536..=4294967295 as `0xFC` + u32 LE; larger as `0xFD` + u64 LE.
 *   (`usize` lengths use the u64 form, enum variant indices the u32 form.)
 * - structs in declaration order with no field tags,
 * - `String` / `Vec<u8>` as a u64-form byte length followed by bytes,
 * - `[u8; 4]` as four raw bytes,
 * - `bool` as one byte (0/1),
 * - `f64` as eight little-endian IEEE-754 bytes.
 *
 * Source of truth: bincode 2.0.1 `src/varint/encode_unsigned.rs` and
 * `src/features/serde/ser.rs`; verified against the project's Cargo.lock.
 */
class BincodeWriter {
    private val out = ByteArrayOutputStream()

    fun u8(value: Int) {
        require(value in 0..0xFF) { "u8 out of range: $value" }
        out.write(value)
    }

    fun bool(value: Boolean) {
        out.write(if (value) 1 else 0)
    }

    fun u16(value: Int) {
        require(value in 0..0xFFFF) { "u16 out of range: $value" }
        if (value <= SINGLE_BYTE_MAX) {
            out.write(value)
        } else {
            out.write(MARKER_U16)
            writeU16Le(value)
        }
    }

    fun u32(value: Long) {
        require(value in 0..0xFFFF_FFFFL) { "u32 out of range: $value" }
        when {
            value <= SINGLE_BYTE_MAX.toLong() -> out.write(value.toInt())
            value <= 0xFFFFL -> {
                out.write(MARKER_U16)
                writeU16Le(value.toInt())
            }

            else -> {
                out.write(MARKER_U32)
                writeU32Le(value)
            }
        }
    }

    fun u64(value: Long) {
        u64(value.toULong())
    }

    fun u64(value: ULong) {
        when {
            value <= SINGLE_BYTE_MAX.toULong() -> out.write(value.toInt())
            value <= 0xFFFFuL -> {
                out.write(MARKER_U16)
                writeU16Le(value.toInt())
            }

            value <= 0xFFFF_FFFFuL -> {
                out.write(MARKER_U32)
                writeU32Le(value.toLong())
            }

            else -> {
                out.write(MARKER_U64)
                writeU64Le(value.toLong())
            }
        }
    }

    fun raw(bytes: ByteArray) {
        out.write(bytes)
    }

    fun f64(value: Double) {
        val bits = value.toRawBits()
        for (shift in 0 until 8) {
            out.write(((bits ushr (shift * 8)) and 0xFF).toInt())
        }
    }

    fun string(value: String) {
        val bytes = value.toByteArray(Charsets.UTF_8)
        require(bytes.size <= Wire.MAX_STRING_BYTES) {
            "string too large: ${bytes.size} bytes"
        }
        u64(bytes.size.toULong())
        out.write(bytes)
    }

    /** `Vec<u8>` / `[u8]`: a u64-form length then the raw bytes. */
    fun bytes(value: ByteArray) {
        u64(value.size.toULong())
        out.write(value)
    }

    fun toByteArray(): ByteArray = out.toByteArray()

    private fun writeU16Le(value: Int) {
        out.write(value and 0xFF)
        out.write((value ushr 8) and 0xFF)
    }

    private fun writeU32Le(value: Long) {
        for (shift in 0 until 4) {
            out.write(((value ushr (shift * 8)) and 0xFF).toInt())
        }
    }

    private fun writeU64Le(value: Long) {
        for (shift in 0 until 8) {
            out.write(((value ushr (shift * 8)) and 0xFF).toInt())
        }
    }

    private companion object {
        const val SINGLE_BYTE_MAX = 250
        const val MARKER_U16 = 251
        const val MARKER_U32 = 252
        const val MARKER_U64 = 253
    }
}

/**
 * Decoder counterpart of [BincodeWriter]. Each unsigned width is decoded with
 * its own marker rules, matching bincode's typed decoders (a u32 marker is
 * invalid where a u16 is expected, etc.).
 */
class BincodeReader(private val data: ByteArray) {
    var position: Int = 0
        private set

    val remaining: Int
        get() = data.size - position

    fun u8(): Int {
        if (position >= data.size) throw WireFormatException("数据不完整")
        return data[position++].toInt() and 0xFF
    }

    fun bool(): Boolean = when (val value = u8()) {
        0 -> false
        1 -> true
        else -> throw WireFormatException("非法布尔值：$value")
    }

    fun u16(): Int {
        val first = u8()
        return when {
            first <= SINGLE_BYTE_MAX -> first
            first == MARKER_U16 -> readU16Le()
            first == MARKER_U32 -> throw WireFormatException("u16 收到 u32 标记")
            first == MARKER_U64 -> throw WireFormatException("u16 收到 u64 标记")
            first == MARKER_U128 -> throw WireFormatException("u16 收到 u128 标记")
            else -> throw WireFormatException("非法 varint 标记：$first")
        }
    }

    fun u32(): Long {
        val first = u8()
        return when {
            first <= SINGLE_BYTE_MAX -> first.toLong()
            first == MARKER_U16 -> readU16Le().toLong()
            first == MARKER_U32 -> readU32Le()
            first == MARKER_U64 -> throw WireFormatException("u32 收到 u64 标记")
            first == MARKER_U128 -> throw WireFormatException("u32 收到 u128 标记")
            else -> throw WireFormatException("非法 varint 标记：$first")
        }
    }

    fun enumIndex(): Int {
        val value = u32()
        if (value > Int.MAX_VALUE) throw WireFormatException("枚举序号越界：$value")
        return value.toInt()
    }

    fun u64(): Long = u64Value().toLong()

    /** Used for `usize` lengths (String / Vec). */
    fun u64Value(): ULong {
        val first = u8()
        return when {
            first <= SINGLE_BYTE_MAX -> first.toULong()
            first == MARKER_U16 -> readU16Le().toULong()
            first == MARKER_U32 -> readU32Le().toULong()
            first == MARKER_U64 -> readU64Le()
            first == MARKER_U128 -> throw WireFormatException("u64 不支持 u128 标记")
            else -> throw WireFormatException("非法 varint 标记：$first")
        }
    }

    fun f64(): Double {
        if (remaining < 8) throw WireFormatException("f64 数据不完整")
        var bits = 0L
        for (shift in 0 until 8) {
            bits = bits or ((data[position++].toLong() and 0xFFL) shl (shift * 8))
        }
        return Double.fromBits(bits)
    }

    fun bytes(maxBytes: Int = Wire.MAX_STRING_BYTES): ByteArray {
        val length = u64Value()
        if (length > maxBytes.toULong()) {
            throw WireFormatException("字节数组过大：$length")
        }
        if (length > remaining.toULong()) {
            throw WireFormatException("字节数组长度超过剩余数据")
        }
        val size = length.toInt()
        val result = data.copyOfRange(position, position + size)
        position += size
        return result
    }

    fun string(maxBytes: Int = Wire.MAX_STRING_BYTES): String =
        String(bytes(maxBytes), Charsets.UTF_8)

    fun requireFullyConsumed() {
        if (remaining != 0) {
            throw WireFormatException("尾随数据：剩余 $remaining 字节")
        }
    }

    private fun readU16Le(): Int {
        if (remaining < 2) throw WireFormatException("u16 数据不完整")
        val value = (data[position].toInt() and 0xFF) or
            ((data[position + 1].toInt() and 0xFF) shl 8)
        position += 2
        return value
    }

    private fun readU32Le(): Long {
        if (remaining < 4) throw WireFormatException("u32 数据不完整")
        var value = 0L
        for (shift in 0 until 4) {
            value = value or ((data[position++].toLong() and 0xFFL) shl (shift * 8))
        }
        return value
    }

    private fun readU64Le(): ULong {
        if (remaining < 8) throw WireFormatException("u64 数据不完整")
        var value = 0uL
        for (shift in 0 until 8) {
            value = value or ((data[position++].toLong() and 0xFFL).toULong() shl (shift * 8))
        }
        return value
    }

    private companion object {
        const val SINGLE_BYTE_MAX = 250
        const val MARKER_U16 = 251
        const val MARKER_U32 = 252
        const val MARKER_U64 = 253
        const val MARKER_U128 = 254
    }
}
