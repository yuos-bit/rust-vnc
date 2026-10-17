package com.rustvnc.client.wire

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream

/**
 * Pure-JVM tests for the bincode 2 `standard()` codec. No Android classes are
 * touched, so these run with `./gradlew :app:testDebugUnitTest`.
 */
class WireCodecTest {

    // ------------------------------------------------------------------ varints

    @Test
    fun bincodeVariableIntMatchesStandardBytes() {
        // bincode 2.0.1: 0..=250 single byte, then 251/252/253 markers
        // followed by little-endian payloads.
        assertArrayEquals(bytes(0x00), encodeVarint(0uL))
        assertArrayEquals(bytes(0x7F), encodeVarint(127uL))
        assertArrayEquals(bytes(0xFA), encodeVarint(250uL))
        assertArrayEquals(bytes(0xFB, 0xFB, 0x00), encodeVarint(251uL))
        assertArrayEquals(bytes(0xFB, 0x2C, 0x01), encodeVarint(300uL))
        assertArrayEquals(bytes(0xFB, 0xFF, 0xFF), encodeVarint(65_535uL))
        assertArrayEquals(bytes(0xFC, 0x00, 0x00, 0x01, 0x00), encodeVarint(65_536uL))
        assertArrayEquals(bytes(0xFC, 0xFF, 0xFF, 0xFF, 0xFF), encodeVarint(0xFFFF_FFFFuL))
        assertArrayEquals(
            bytes(0xFD, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF),
            encodeVarint(ULong.MAX_VALUE),
        )
    }

    @Test
    fun bincodeVariableIntRoundTripsAtBoundaries() {
        val values = listOf(
            0uL,
            1uL,
            250uL,
            251uL,
            255uL,
            300uL,
            65_535uL,
            65_536uL,
            0xFFFF_FFFFuL,
            ULong.MAX_VALUE,
        )
        for (value in values) {
            val reader = BincodeReader(encodeVarint(value))
            assertTrue(value == reader.u64Value())
            assertEquals(0, reader.remaining)
        }
    }

    @Test
    fun invalidOrTruncatedVariableIntsAreRejected() {
        // Truncated u16 payload.
        assertThrows(WireFormatException::class.java) {
            BincodeReader(bytes(0xFB, 0x01)).u64()
        }
        // A u128 marker is not valid for a u64.
        assertThrows(WireFormatException::class.java) {
            BincodeReader(bytes(0xFE)).u64()
        }
        // Reserved marker 0xFF.
        assertThrows(WireFormatException::class.java) {
            BincodeReader(bytes(0xFF)).u16()
        }
        // A u32 marker is invalid where a u16 is expected.
        assertThrows(WireFormatException::class.java) {
            BincodeReader(bytes(0xFC, 0x01, 0x00, 0x00, 0x00)).u16()
        }
    }

    @Test
    fun typedVariableIntsUseTheirOwnMarkers() {
        val w16 = BincodeWriter().apply { u16(1920) }
        assertArrayEquals(bytes(0xFB, 0x80, 0x07), w16.toByteArray())
        assertEquals(1920, BincodeReader(w16.toByteArray()).u16())

        val w32 = BincodeWriter().apply { u32(70_000) }
        assertArrayEquals(bytes(0xFC, 0x70, 0x11, 0x01, 0x00), w32.toByteArray())
        assertEquals(70_000L, BincodeReader(w32.toByteArray()).u32())
    }

    @Test
    fun stringLengthAbove250UsesTaggedLength() {
        val text = "a".repeat(300)
        val encoded = BincodeWriter().apply { string(text) }.toByteArray()
        assertArrayEquals(bytes(0xFB, 0x2C, 0x01), encoded.copyOfRange(0, 3))
        assertEquals(text, BincodeReader(encoded).string())
    }

    // ---------------------------------------------------------------- fixed f64

    @Test
    fun f64IsLittleEndianIeee754() {
        val writer = BincodeWriter()
        writer.f64(0.25)
        assertArrayEquals(
            bytes(0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xD0, 0x3F),
            writer.toByteArray(),
        )

        val reader = BincodeReader(writer.toByteArray())
        assertEquals(0.25, reader.f64(), 0.0)
    }

    // ------------------------------------------------------------- client hello

    @Test
    fun helloHasExactWireLayout() {
        val message = ClientMessage.Hello(
            deviceCode = "ABC234",
            token = "t",
            controllerId = "c",
            controllerName = "n",
            platform = "android",
        )
        val expected = bytes(
            0x00, // ClientMessage::Hello variant
            0x52, 0x56, 0x4E, 0x43, // "RVNC"
            0x01, // wire_version = 1
            0x06, 0x41, 0x42, 0x43, 0x32, 0x33, 0x34, // "ABC234"
            0x01, 0x74, // "t"
            0x01, 0x63, // "c"
            0x01, 0x6E, // "n"
            0x07, 0x61, 0x6E, 0x64, 0x72, 0x6F, 0x69, 0x64, // "android"
        )
        assertArrayEquals(expected, ClientCodec.encode(message))
    }

    @Test
    fun pingUsesVariantTwoAndVarintId() {
        assertArrayEquals(
            bytes(0x02, 0x2A),
            ClientCodec.encode(ClientMessage.Ping(42L)),
        )
    }

    @Test
    fun byeIsASingleVariantByte() {
        assertArrayEquals(bytes(0x03), ClientCodec.encode(ClientMessage.Bye))
    }

    // ----------------------------------------------------------------- inputs

    @Test
    fun mouseMoveUsesTwoLittleEndianDoubles() {
        val encoded = ClientCodec.encode(
            ClientMessage.Input(InputEvent.MouseMove(0.25, 0.75)),
        )
        val expected = bytes(0x01, 0x00) +
            bytes(0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xD0, 0x3F) +
            bytes(0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xE8, 0x3F)
        assertArrayEquals(expected, encoded)
    }

    @Test
    fun mouseButtonVariantOrderMatchesRustEnum() {
        // rv_proto::MouseButton is declared Left, Right, Middle, Back, Forward.
        assertArrayEquals(bytes(0x01, 0x01, 0x00, 0x01), encodeButton(MouseWireButton.Left))
        assertArrayEquals(bytes(0x01, 0x01, 0x01, 0x01), encodeButton(MouseWireButton.Right))
        assertArrayEquals(bytes(0x01, 0x01, 0x02, 0x01), encodeButton(MouseWireButton.Middle))
        assertArrayEquals(bytes(0x01, 0x01, 0x03, 0x01), encodeButton(MouseWireButton.Back))
        assertArrayEquals(bytes(0x01, 0x01, 0x04, 0x01), encodeButton(MouseWireButton.Forward))
    }

    @Test
    fun wheelAndKeyLayoutsMatchRustFieldOrder() {
        val wheel = ClientCodec.encode(
            ClientMessage.Input(InputEvent.Wheel(0.0, -1.0)),
        )
        assertEquals(0x01, wheel[0].toInt())
        assertEquals(0x02, wheel[1].toInt())
        assertEquals(18, wheel.size) // 2 variant bytes + 2 * f64

        val key = ClientCodec.encode(
            ClientMessage.Input(InputEvent.Key("KeyA", pressed = true)),
        )
        assertArrayEquals(
            bytes(0x01, 0x03, 0x04, 0x4B, 0x65, 0x79, 0x41, 0x01),
            key,
        )
    }

    @Test
    fun textLengthCountsUtf8BytesNotCharacters() {
        val encoded = ClientCodec.encode(
            ClientMessage.Input(InputEvent.Text("你好")),
        )
        assertArrayEquals(
            bytes(
                0x01, 0x04, // Input, Text variant
                0x06, // six UTF-8 bytes
                0xE4, 0xBD, 0xA0, 0xE5, 0xA5, 0xBD,
            ),
            encoded,
        )
    }

    // -------------------------------------------------------------- server side

    @Test
    fun serverHelloDecodes() {
        val writer = BincodeWriter()
        writer.u32(0L)
        writer.u16(1)
        writer.bool(true)
        writer.string("sess-1")
        writer.string("工作站")
        writer.u32(1920)
        writer.u32(1080)
        writer.u16(15)

        val message = ServerCodec.decode(writer.toByteArray())
        assertTrue(message is ServerMessage.Hello)
        val hello = (message as ServerMessage.Hello).hello
        assertEquals(1, hello.wireVersion)
        assertTrue(hello.accepted)
        assertEquals("sess-1", hello.sessionId)
        assertEquals("工作站", hello.deviceName)
        assertEquals(1920L, hello.width)
        assertEquals(1080L, hello.height)
        assertEquals(15, hello.fps)
    }

    @Test
    fun videoFrameDecodes() {
        val writer = BincodeWriter()
        writer.u32(1L)
        writer.u64(7)
        writer.u32(800)
        writer.u32(600)
        writer.u64(123_456)
        writer.u32(0L) // FrameFormat::Jpeg
        writer.bytes(bytes(0xFF, 0xD8, 0xFF, 0xD9))

        val message = ServerCodec.decode(writer.toByteArray())
        assertTrue(message is ServerMessage.Video)
        val video = (message as ServerMessage.Video).frame
        assertEquals(7L, video.seq)
        assertEquals(800L, video.width)
        assertEquals(600L, video.height)
        assertEquals(123_456L, video.timestampMs)
        assertEquals(FrameFormat.Jpeg, video.format)
        assertArrayEquals(bytes(0xFF, 0xD8, 0xFF, 0xD9), video.data)
    }

    @Test
    fun videoFrameWithLargePayloadDecodes() {
        val payload = ByteArray(70_000) { index -> (index and 0xFF).toByte() }
        val writer = BincodeWriter()
        writer.u32(1L)
        writer.u64(1)
        writer.u32(1920)
        writer.u32(1080)
        writer.u64(1_700_000_000_000)
        writer.u32(0L) // FrameFormat::Jpeg
        writer.bytes(payload)

        val message = ServerCodec.decode(writer.toByteArray())
        val video = (message as ServerMessage.Video).frame
        assertEquals(70_000, video.data.size)
        assertArrayEquals(payload, video.data)
        assertEquals(1_700_000_000_000L, video.timestampMs)
    }

    @Test
    fun pongAndErrorDecode() {
        val pong = BincodeWriter().apply {
            u32(2L)
            u64(99)
        }
        val pongMessage = ServerCodec.decode(pong.toByteArray())
        assertEquals(99L, (pongMessage as ServerMessage.Pong).id)

        val error = BincodeWriter().apply {
            u32(3L)
            string("auth_failed")
            string("令牌已过期")
        }
        val errorMessage = ServerCodec.decode(error.toByteArray())
        val body = (errorMessage as ServerMessage.Error).body
        assertEquals("auth_failed", body.code)
        assertEquals("令牌已过期", body.message)
    }

    @Test
    fun trailingServerBytesAreRejected() {
        assertThrows(WireFormatException::class.java) {
            ServerCodec.decode(bytes(0x02, 0x05, 0x00))
        }
    }

    @Test
    fun unknownServerVariantIsRejected() {
        assertThrows(WireFormatException::class.java) {
            ServerCodec.decode(bytes(0x7F))
        }
    }

    // ------------------------------------------------------------------ framing

    @Test
    fun framePrefixIsU32LittleEndian() {
        val output = ByteArrayOutputStream()
        FrameCodec.write(output, bytes(0xAA, 0xBB, 0xCC))
        assertArrayEquals(
            bytes(0x03, 0x00, 0x00, 0x00, 0xAA, 0xBB, 0xCC),
            output.toByteArray(),
        )

        val read = FrameCodec.read(ByteArrayInputStream(output.toByteArray()))
        assertArrayEquals(bytes(0xAA, 0xBB, 0xCC), read!!)
    }

    @Test
    fun cleanEofReturnsNull() {
        assertNull(FrameCodec.read(ByteArrayInputStream(ByteArray(0))))
    }

    @Test
    fun zeroLengthAndOversizedFramesAreRejected() {
        assertThrows(WireFormatException::class.java) {
            FrameCodec.read(ByteArrayInputStream(bytes(0, 0, 0, 0)))
        }
        // 16 MiB + 1 = 0x01000001, little endian.
        assertThrows(WireFormatException::class.java) {
            FrameCodec.read(ByteArrayInputStream(bytes(0x01, 0x00, 0x00, 0x01)))
        }
    }

    // ------------------------------------------------------------------ helpers

    private fun encodeButton(button: MouseWireButton): ByteArray =
        ClientCodec.encode(
            ClientMessage.Input(
                InputEvent.MouseButton(button, pressed = true),
            ),
        )

    private fun encodeVarint(value: ULong): ByteArray {
        val writer = BincodeWriter()
        writer.u64(value)
        return writer.toByteArray()
    }

    private fun bytes(vararg values: Int): ByteArray =
        ByteArray(values.size) { index -> values[index].toByte() }
}
