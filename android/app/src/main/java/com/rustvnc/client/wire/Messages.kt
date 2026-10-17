package com.rustvnc.client.wire

/**
 * Mirrors `rv_proto::ClientMessage` in `crates/rv-proto/src/wire.rs`.
 */
sealed interface ClientMessage {
    data class Hello(
        val deviceCode: String,
        val token: String,
        val controllerId: String,
        val controllerName: String,
        val platform: String = "android",
    ) : ClientMessage

    data class Input(val event: InputEvent) : ClientMessage

    data class Ping(val id: Long) : ClientMessage

    data object Bye : ClientMessage
}

/**
 * Mirrors `rv_proto::InputEvent`.
 */
sealed interface InputEvent {
    data class MouseMove(val x: Double, val y: Double) : InputEvent

    data class MouseButton(val button: MouseWireButton, val pressed: Boolean) : InputEvent

    data class Wheel(val deltaX: Double, val deltaY: Double) : InputEvent

    data class Key(val code: String, val pressed: Boolean) : InputEvent

    data class Text(val text: String) : InputEvent

    data class Clipboard(val text: String) : InputEvent
}

/**
 * Variant indices MUST match the declaration order of
 * `rv_proto::MouseButton`:
 *
 * ```rust
 * pub enum MouseButton { Left, Right, Middle, Back, Forward }
 * ```
 *
 * so Left=0, Right=1, Middle=2, Back=3, Forward=4. Do not reorder: bincode
 * encodes the enum variant index.
 */
enum class MouseWireButton(val wireIndex: Int) {
    Left(0),
    Right(1),
    Middle(2),
    Back(3),
    Forward(4),
}

/**
 * Mirrors `rv_proto::ServerMessage`.
 */
sealed interface ServerMessage {
    data class Hello(val hello: ServerHello) : ServerMessage

    data class Video(val frame: VideoFrame) : ServerMessage

    data class Pong(val id: Long) : ServerMessage

    data class Error(val body: ServerError) : ServerMessage
}

data class ServerHello(
    val wireVersion: Int,
    val accepted: Boolean,
    val sessionId: String,
    val deviceName: String,
    val width: Long,
    val height: Long,
    val fps: Int,
)

data class VideoFrame(
    val seq: Long,
    val width: Long,
    val height: Long,
    val timestampMs: Long,
    val format: FrameFormat,
    val data: ByteArray,
)

enum class FrameFormat(val wireIndex: Int) {
    Jpeg(0),
}

data class ServerError(
    val code: String,
    val message: String,
)
