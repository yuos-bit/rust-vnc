package com.rustvnc.client.ui.pairing

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.rustvnc.client.data.DeviceCode
import com.rustvnc.client.data.PairOutcome
import com.rustvnc.client.data.PairingRepository
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

class PairingViewModel(
    private val repository: PairingRepository,
) : ViewModel() {

    data class UiState(
        val cells: List<Char?> = List(DeviceCode.LENGTH) { null },
        val password: String = "",
        val passwordVisible: Boolean = false,
        val loading: Boolean = false,
        val error: String? = null,
    ) {
        val code: String
            get() = buildString {
                cells.forEach { cell -> if (cell != null) append(cell) }
            }

        val complete: Boolean
            get() = cells.all { it != null }
    }

    private val _state = MutableStateFlow(UiState())
    val state: StateFlow<UiState> = _state.asStateFlow()

    private val _paired = MutableStateFlow<PairOutcome?>(null)
    val paired: StateFlow<PairOutcome?> = _paired.asStateFlow()

    val serverUrl: StateFlow<String> = repository.serverUrlFlow

    fun updateCells(cells: List<Char?>) {
        val normalized = List(DeviceCode.LENGTH) { index -> cells.getOrNull(index) }
        _state.update { it.copy(cells = normalized, error = null) }
    }

    fun setPassword(value: String) {
        _state.update { it.copy(password = value.take(64), error = null) }
    }

    fun togglePasswordVisible() {
        _state.update { it.copy(passwordVisible = !it.passwordVisible) }
    }

    fun clearError() {
        _state.update { it.copy(error = null) }
    }

    /**
     * Runs the pair + connect flow. On success the outcome is published on
     * [paired]; the screen navigates from a LaunchedEffect, so a request that
     * finishes while another screen is on top does not pop the wrong entry.
     */
    fun pair() {
        val snapshot = _state.value
        if (snapshot.loading) return

        if (!DeviceCode.isValid(snapshot.code)) {
            _state.update { it.copy(error = "请输入完整的 6 位设备码") }
            return
        }
        if (snapshot.password.isEmpty()) {
            _state.update { it.copy(error = "请输入配对密码") }
            return
        }

        _state.update { it.copy(loading = true, error = null) }
        viewModelScope.launch {
            try {
                val outcome = repository.pair(snapshot.code, snapshot.password)
                _state.update { it.copy(loading = false, password = "") }
                _paired.value = outcome
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Exception) {
                _state.update {
                    it.copy(
                        loading = false,
                        error = error.message?.takeIf(String::isNotBlank) ?: "配对失败，请稍后重试",
                    )
                }
            }
        }
    }

    fun consumePaired() {
        _paired.value = null
    }
}
