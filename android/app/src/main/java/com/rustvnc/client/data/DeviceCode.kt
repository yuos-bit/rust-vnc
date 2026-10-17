package com.rustvnc.client.data

/**
 * Mirror of `rv_core::code`: six characters from an unambiguous alphabet
 * (no 0/O, no 1/I/L). The server normalizes separators, but the Android UI
 * restricts input up front so the six boxes cannot hold invalid characters.
 */
object DeviceCode {
    const val LENGTH = 6
    const val DEFAULT_PORT = 21416
    const val ALPHABET = "23456789ABCDEFGHJKLMNPQRSTUVWXYZ"

    fun sanitizeChar(char: Char): Char? {
        val upper = char.uppercaseChar()
        return upper.takeIf { it in ALPHABET }
    }

    /** Filters and uppercases, keeping at most [LENGTH] characters. */
    fun sanitize(raw: String): String {
        val builder = StringBuilder(LENGTH)
        for (char in raw) {
            val valid = sanitizeChar(char) ?: continue
            builder.append(valid)
            if (builder.length == LENGTH) break
        }
        return builder.toString()
    }

    fun isValid(code: String): Boolean =
        code.length == LENGTH && code.all { it in ALPHABET }

    fun display(code: String): String = sanitize(code)
}
