package com.rustvnc.client.wire

/**
 * Encodes the controller -> controlled-endpoint messages using the exact
 * bincode 2 `standard()` layout used by `rv_proto::encode`.
 *
 * Clients never decode [ClientMessage], so only an encoder is provided.
 */
object ClientCodec {

    fun encode(message: ClientMessage): ByteArray {
        val writer = BincodeWriter()
        when (message) {
            is ClientMessage.Hello -> {
                writer.u32(VARIANT_HELLO)
                writer.raw(Wire.MAGIC.toByteArray(Charsets.US_ASCII))
                writer.u16(Wire.WIRE_VERSION)
                writer.string(message.deviceCode)
                writer.string(message.token)
                writer.string(message.controllerId)
                writer.string(message.controllerName)
                writer.string(message.platform)
            }

            is ClientMessage.Input -> {
                writer.u32(VARIANT_INPUT)
                encodeInput(writer, message.event)
            }

            is ClientMessage.Ping -> {
                writer.u32(VARIANT_PING)
                writer.u64(message.id)
            }

            ClientMessage.Bye -> {
                writer.u32(VARIANT_BYE)
            }
        }
        return writer.toByteArray()
    }

    private fun encodeInput(writer: BincodeWriter, event: InputEvent) {
        when (event) {
            is InputEvent.MouseMove -> {
                writer.u32(INPUT_MOUSE_MOVE)
                writer.f64(event.x)
                writer.f64(event.y)
            }

            is InputEvent.MouseButton -> {
                writer.u32(INPUT_MOUSE_BUTTON)
                writer.u32(event.button.wireIndex.toLong())
                writer.bool(event.pressed)
            }

            is InputEvent.Wheel -> {
                writer.u32(INPUT_WHEEL)
                writer.f64(event.deltaX)
                writer.f64(event.deltaY)
            }

            is InputEvent.Key -> {
                writer.u32(INPUT_KEY)
                writer.string(event.code)
                writer.bool(event.pressed)
            }

            is InputEvent.Text -> {
                writer.u32(INPUT_TEXT)
                writer.string(event.text)
            }

            is InputEvent.Clipboard -> {
                writer.u32(INPUT_CLIPBOARD)
                writer.string(event.text)
            }
        }
    }

    // ClientMessage variant indices (u32 variable-int form).
    private const val VARIANT_HELLO = 0L
    private const val VARIANT_INPUT = 1L
    private const val VARIANT_PING = 2L
    private const val VARIANT_BYE = 3L

    // InputEvent variant indices.
    private const val INPUT_MOUSE_MOVE = 0L
    private const val INPUT_MOUSE_BUTTON = 1L
    private const val INPUT_WHEEL = 2L
    private const val INPUT_KEY = 3L
    private const val INPUT_TEXT = 4L
    private const val INPUT_CLIPBOARD = 5L
}
