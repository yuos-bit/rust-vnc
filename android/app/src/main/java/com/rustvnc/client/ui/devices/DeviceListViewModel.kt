package com.rustvnc.client.ui.devices

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.rustvnc.client.data.PairingRepository
import com.rustvnc.client.data.SavedDevice
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.supervisorScope

class DeviceListViewModel(
    private val repository: PairingRepository,
) : ViewModel() {

    val devices: StateFlow<List<SavedDevice>> = repository.devices

    private val _refreshing = MutableStateFlow(false)
    val refreshing: StateFlow<Boolean> = _refreshing.asStateFlow()

    private val _message = MutableStateFlow<String?>(null)
    val message: StateFlow<String?> = _message.asStateFlow()

    /**
     * Resolves every saved device through /api/v1/connect in parallel. This
     * also refreshes the online dot and the tailnet address/port.
     */
    fun refresh() {
        if (_refreshing.value) return
        val snapshot = devices.value
        if (snapshot.isEmpty()) return

        _refreshing.value = true
        viewModelScope.launch {
            val successes = supervisorScope {
                snapshot.map { device ->
                    async {
                        try {
                            repository.connect(device)
                            true
                        } catch (cancelled: CancellationException) {
                            throw cancelled
                        } catch (_: Exception) {
                            false
                        }
                    }
                }.map { it.await() }
            }
            _refreshing.value = false
            _message.value = when {
                successes.all { it } -> "设备状态已刷新"
                successes.none { it } -> "无法连接配对服务器，或设备均离线"
                else -> "部分设备离线或令牌已失效"
            }
        }
    }

    fun removeLocal(deviceCode: String) {
        repository.removeLocal(deviceCode)
        _message.value = "已移除本机配对信息；服务器端令牌未被吊销"
    }

    fun consumeMessage() {
        _message.value = null
    }
}
