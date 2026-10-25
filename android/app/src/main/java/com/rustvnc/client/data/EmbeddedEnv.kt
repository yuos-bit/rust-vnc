package com.rustvnc.client.data

import android.content.Context

/**
 * Configuration embedded into the APK at build time.
 *
 * The Gradle `embedEnv` task copies the repository-root `.env` to
 * `assets/embedded_env.txt`; CI builds without a `.env` get an empty asset.
 * Supports `KEY=value`, `KEY:value` and the full-width Chinese colon
 * (`KEY：value`) used by the local deployment file.
 *
 * The values are inside the APK, so anyone with the APK can extract them.
 * Never distribute a build containing production secrets.
 */
object EmbeddedEnv {
    @Volatile
    private var cached: List<Pair<String, String>> = emptyList()

    fun init(context: Context) {
        cached = runCatching {
            context.assets.open("embedded_env.txt").bufferedReader().use { it.readText() }
        }.getOrDefault("").let(::parse)
    }

    fun parse(text: String): List<Pair<String, String>> =
        text.lineSequence()
            .map { it.trim() }
            .filter { it.isNotEmpty() && !it.startsWith("#") }
            .mapNotNull { line ->
                val index = line.indexOfAny(charArrayOf('=', '：', ':'))
                if (index <= 0) {
                    null
                } else {
                    val key = line.substring(0, index).trim()
                    val value = line.substring(index + 1).trim()
                    if (key.isEmpty()) null else key to value
                }
            }
            .toList()

    fun entries(): List<Pair<String, String>> = cached

    /** First value whose key matches one of [keys] (case-insensitive). */
    fun value(vararg keys: String): String? {
        val wanted = keys.map { it.lowercase() }.toSet()
        return cached.firstOrNull { it.first.lowercase() in wanted }?.second
    }
}
