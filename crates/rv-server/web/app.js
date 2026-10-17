/* RustVNC administration console.
   Plain ES2022, no build step, served by rv-server. */
(() => {
  'use strict';

  const TOKEN_KEY = 'rvnc_admin_token';
  const $ = (selector, root = document) => root.querySelector(selector);
  const $$ = (selector, root = document) => Array.from(root.querySelectorAll(selector));

  const state = {
    token: sessionStorage.getItem(TOKEN_KEY) || '',
    devices: [],
    tokens: [],
    pairings: [],
    pairingLimit: 100,
    view: 'devices',
    busy: false,
  };

  // ------------------------------------------------------------------ utils
  function escapeHtml(value) {
    return String(value ?? '')
      .replaceAll('&', '&amp;')
      .replaceAll('<', '&lt;')
      .replaceAll('>', '&gt;')
      .replaceAll('"', '&quot;')
      .replaceAll("'", '&#39;');
  }

  function formatTime(seconds) {
    if (!seconds) return '—';
    const date = new Date(Number(seconds) * 1000);
    if (Number.isNaN(date.getTime())) return '—';
    return date.toLocaleString('zh-CN', {
      year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit', hour12: false,
    });
  }

  function relativeTime(seconds) {
    if (!seconds) return '从未';
    const delta = Math.max(0, Math.floor(Date.now() / 1000 - Number(seconds)));
    if (delta < 60) return '刚刚';
    if (delta < 3600) return `${Math.floor(delta / 60)} 分钟前`;
    if (delta < 86400) return `${Math.floor(delta / 3600)} 小时前`;
    return `${Math.floor(delta / 86400)} 天前`;
  }

  let toastTimer = null;
  function toast(message, isError = false) {
    const element = $('#toast');
    element.textContent = message;
    element.classList.toggle('is-error', isError);
    element.classList.add('is-visible');
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => element.classList.remove('is-visible'), 3200);
  }

  // -------------------------------------------------------------------- api
  async function api(path, options = {}) {
    const headers = Object.assign(
      { 'Content-Type': 'application/json' },
      options.headers || {},
    );
    if (state.token) headers.Authorization = `Bearer ${state.token}`;
    const response = await fetch(path, { ...options, headers });
    let payload = null;
    try {
      payload = await response.json();
    } catch {
      payload = null;
    }
    if (response.status === 401 && !path.endsWith('/login')) {
      showLogin('登录已失效，请重新输入管理员令牌');
      throw new Error('未授权');
    }
    if (!response.ok) {
      throw new Error(payload?.error?.message || `请求失败（HTTP ${response.status}）`);
    }
    return payload;
  }

  // ------------------------------------------------------------------ views
  function switchView(name) {
    state.view = name;
    $$('.view').forEach((view) => view.classList.toggle('is-visible', view.id === `view-${name}`));
    $$('.nav-link').forEach((link) => link.classList.toggle('is-active', link.dataset.view === name));
    if (name === 'settings') loadSettings();
    if (name === 'pairings') loadPairings();
    if (name === 'tokens') loadTokens();
    if (name === 'devices') loadDevices();
  }

  // ------------------------------------------------------------------ login
  function showLogin(message = '') {
    const overlay = $('#loginOverlay');
    overlay.classList.remove('is-hidden');
    const error = $('#loginError');
    error.hidden = !message;
    error.textContent = message;
    const input = $('#adminTokenInput');
    input.value = '';
    setTimeout(() => input.focus(), 120);
  }

  function hideLogin() {
    $('#loginOverlay').classList.add('is-hidden');
    $('#loginError').hidden = true;
  }

  async function handleLogin(event) {
    event.preventDefault();
    const button = $('#loginBtn');
    const token = $('#adminTokenInput').value.trim();
    if (!token) return;
    button.disabled = true;
    button.textContent = '验证中…';
    try {
      await api('/api/admin/login', {
        method: 'POST',
        headers: { Authorization: '' },
        body: JSON.stringify({ token }),
      });
      state.token = token;
      sessionStorage.setItem(TOKEN_KEY, token);
      hideLogin();
      toast('已进入管理后台');
      await refreshAll();
    } catch (error) {
      const box = $('#loginError');
      box.hidden = false;
      box.textContent = error.message || '登录失败';
    } finally {
      button.disabled = false;
      button.textContent = '进入控制台';
    }
  }

  // ------------------------------------------------------------------ data
  async function refreshAll() {
    if (state.busy) return;
    state.busy = true;
    try {
      await Promise.allSettled([loadStats(), loadDevices(), loadTokens(), loadPairings()]);
      if (state.view === 'settings') await loadSettings();
    } finally {
      state.busy = false;
    }
  }

  async function loadStats() {
    try {
      const stats = await api('/api/admin/stats');
      $('#statOnline').textContent = stats.devices_online ?? 0;
      $('#statTotal').textContent = `共 ${stats.devices_total ?? 0} 台`;
      $('#statTokens').textContent = stats.active_tokens ?? 0;
      $('#statPairings').textContent = stats.pairings_today ?? 0;
    } catch (error) {
      if (!String(error.message).includes('未授权')) toast(error.message, true);
    }
  }

  async function loadDevices() {
    try {
      state.devices = await api('/api/admin/devices');
      renderDevices();
    } catch (error) {
      if (!String(error.message).includes('未授权')) toast(error.message, true);
    }
  }

  function renderDevices() {
    const filter = ($('#deviceFilter').value || '').trim().toLowerCase();
    const rows = state.devices.filter((device) => {
      if (!filter) return true;
      return [device.code, device.name, device.tailscale_ip, device.platform]
        .filter(Boolean)
        .some((value) => String(value).toLowerCase().includes(filter));
    });

    const body = $('#devicesBody');
    body.innerHTML = rows.map((device) => {
      const online = device.online;
      return `
        <tr>
          <td><span class="status"><i class="dot ${online ? 'online' : 'offline'}"></i>${online ? '在线' : '离线'}</span></td>
          <td class="mono">${escapeHtml(device.code)}</td>
          <td>${escapeHtml(device.name)}</td>
          <td><span class="tag">${escapeHtml(device.platform)}</span></td>
          <td class="mono dim">${escapeHtml(device.tailscale_ip || '未加入')}</td>
          <td>${Number(device.paired_controllers) || 0}</td>
          <td class="dim" title="${escapeHtml(formatTime(device.last_seen))}">${escapeHtml(relativeTime(device.last_seen))}</td>
          <td class="right"><button class="link-btn" type="button" data-delete-device="${escapeHtml(device.code)}">删除</button></td>
        </tr>`;
    }).join('');

    $('#devicesEmpty').hidden = rows.length !== 0;
    $('#deviceCount').textContent = filter
      ? `${rows.length} / ${state.devices.length} 台`
      : `${state.devices.length} 台设备`;
  }

  async function loadTokens() {
    try {
      state.tokens = await api('/api/admin/tokens');
      renderTokens();
    } catch (error) {
      if (!String(error.message).includes('未授权')) toast(error.message, true);
    }
  }

  function renderTokens() {
    const now = Math.floor(Date.now() / 1000);
    const body = $('#tokensBody');
    body.innerHTML = state.tokens.map((token) => {
      let status = '<span class="tag ok">有效</span>';
      let dot = 'online';
      if (token.revoked_at) {
        status = '<span class="tag bad">已吊销</span>';
        dot = 'revoked';
      } else if (Number(token.expires_at) <= now) {
        status = '<span class="tag warn">已过期</span>';
        dot = 'offline';
      }
      const action = token.revoked_at
        ? '<span class="dim">—</span>'
        : `<button class="link-btn" type="button" data-revoke-token="${escapeHtml(token.id)}">吊销</button>`;
      return `
        <tr>
          <td><span class="status"><i class="dot ${dot}"></i>${status}</span></td>
          <td><span class="mono">${escapeHtml(token.device_code)}</span> <span class="dim">${escapeHtml(token.device_name)}</span></td>
          <td>${escapeHtml(token.controller_name)}</td>
          <td><span class="tag">${escapeHtml(token.platform)}</span></td>
          <td class="dim">${escapeHtml(formatTime(token.created_at))}</td>
          <td class="dim">${escapeHtml(formatTime(token.expires_at))}</td>
          <td class="dim">${escapeHtml(relativeTime(token.last_used))}</td>
          <td class="right">${action}</td>
        </tr>`;
    }).join('');
    $('#tokensEmpty').hidden = state.tokens.length !== 0;
  }

  async function loadPairings() {
    try {
      state.pairings = await api(`/api/admin/pairings?limit=${state.pairingLimit}`);
      renderPairings();
    } catch (error) {
      if (!String(error.message).includes('未授权')) toast(error.message, true);
    }
  }

  function renderPairings() {
    const body = $('#pairingsBody');
    body.innerHTML = state.pairings.map((pairing) => `
      <tr>
        <td class="dim">${escapeHtml(formatTime(pairing.created_at))}</td>
        <td class="mono">${escapeHtml(pairing.device_code || '—')}</td>
        <td>${escapeHtml(pairing.controller_name || '—')}</td>
        <td class="mono dim">${escapeHtml(pairing.ip || '—')}</td>
        <td>${pairing.success ? '<span class="tag ok">成功</span>' : '<span class="tag bad">失败</span>'}</td>
        <td class="dim">${escapeHtml(reasonText(pairing.reason))}</td>
      </tr>`).join('');
    $('#pairingsEmpty').hidden = state.pairings.length !== 0;
  }

  function reasonText(reason) {
    if (!reason) return '—';
    return {
      invalid_code: '设备码格式错误',
      unknown_device: '设备不存在',
      bad_password: '密码错误',
      rate_limited: '触发频率限制',
    }[reason] || reason;
  }

  // ---------------------------------------------------------------- settings
  async function loadSettings() {
    try {
      const [settings, info] = await Promise.all([
        api('/api/admin/settings'),
        api('/api/v1/info').catch(() => null),
      ]);
      state.settings = settings;
      $('#headscaleUrl').value = settings.headscale_url || '';
      $('#headscaleApiKey').value = '';
      $('#headscaleApiKey').placeholder = settings.headscale_api_key_set
        ? '已配置，留空保持不变'
        : '输入 Headscale API Key';
      $('#headscaleUser').value = settings.headscale_user || '1';
      $('#derpUrl').value = settings.derp_url || '';
      $('#publicUrl').value = settings.public_url || '';
      $('#agentPort').value = settings.agent_port || 21416;
      $('#tokenTtl').value = settings.token_ttl_days || 30;

      const tag = $('#apiKeyState');
      tag.textContent = settings.headscale_api_key_set ? '已配置' : '未配置';
      tag.className = `tag ${settings.headscale_api_key_set ? 'ok' : 'warn'}`;

      if (info) {
        $('#aboutVersion').textContent = `v${info.version}`;
        $('#aboutProtocol').textContent = `v${info.protocol_version}`;
        $('#aboutHeadscale').textContent = info.headscale_configured ? '已连接配置' : '未配置';
      }
    } catch (error) {
      if (!String(error.message).includes('未授权')) toast(error.message, true);
    }
  }

  async function saveSettings(event) {
    event.preventDefault();
    const button = $('#saveSettings');
    button.disabled = true;
    const payload = {
      headscale_url: $('#headscaleUrl').value.trim(),
      headscale_user: $('#headscaleUser').value.trim(),
      derp_url: $('#derpUrl').value.trim(),
      public_url: $('#publicUrl').value.trim(),
      agent_port: Number($('#agentPort').value) || 21416,
      token_ttl_days: Number($('#tokenTtl').value) || 30,
    };
    const apiKey = $('#headscaleApiKey').value.trim();
    if (apiKey) payload.headscale_api_key = apiKey;

    try {
      const settings = await api('/api/admin/settings', {
        method: 'PUT',
        body: JSON.stringify(payload),
      });
      state.settings = settings;
      $('#headscaleApiKey').value = '';
      toast('设置已保存');
      await loadSettings();
    } catch (error) {
      toast(error.message, true);
    } finally {
      button.disabled = false;
    }
  }

  async function clearApiKey() {
    if (!window.confirm('确定清除已保存的 Headscale API Key？')) return;
    try {
      await api('/api/admin/settings', {
        method: 'PUT',
        body: JSON.stringify({ headscale_api_key: '' }),
      });
      toast('API Key 已清除');
      await loadSettings();
    } catch (error) {
      toast(error.message, true);
    }
  }

  async function testNodes() {
    const box = $('#nodesList');
    box.hidden = false;
    box.innerHTML = '<p class="hint">正在查询 Headscale 节点…</p>';
    try {
      const nodes = await api('/api/admin/headscale/nodes');
      if (!nodes.length) {
        box.innerHTML = '<p class="hint">Headscale 已连接，但还没有节点。</p>';
        return;
      }
      box.innerHTML = nodes.map((node) => `
        <div class="node-row">
          <span>${node.online ? '<i class="dot online"></i> ' : '<i class="dot offline"></i> '}${escapeHtml(node.name || node.id)}</span>
          <span class="ip">${escapeHtml(node.ip_addresses?.[0] || '无地址')}</span>
        </div>`).join('');
    } catch (error) {
      box.innerHTML = `<p class="hint">连接失败：${escapeHtml(error.message)}</p>`;
    }
  }

  // ---------------------------------------------------------------- actions
  async function deleteDevice(code) {
    if (!window.confirm(`删除设备 ${code}？其所有令牌将立即失效。`)) return;
    try {
      await api(`/api/admin/devices/${encodeURIComponent(code)}`, { method: 'DELETE' });
      toast('设备已删除');
      await Promise.all([loadDevices(), loadTokens(), loadStats()]);
    } catch (error) {
      toast(error.message, true);
    }
  }

  async function revokeToken(id) {
    if (!window.confirm('吊销该令牌？该控制端下次连接需要重新配对。')) return;
    try {
      await api(`/api/admin/tokens/${encodeURIComponent(id)}`, { method: 'DELETE' });
      toast('令牌已吊销');
      await Promise.all([loadTokens(), loadStats()]);
    } catch (error) {
      toast(error.message, true);
    }
  }

  function logout() {
    sessionStorage.removeItem(TOKEN_KEY);
    state.token = '';
    state.devices = [];
    state.tokens = [];
    state.pairings = [];
    showLogin('已退出登录');
  }

  // ------------------------------------------------------------------- boot
  function bind() {
    $('#loginForm').addEventListener('submit', handleLogin);
    $('#settingsBtn').addEventListener('click', () => switchView('settings'));
    $('#refreshBtn').addEventListener('click', () => {
      refreshAll();
      toast('正在刷新…');
    });
    $('#deviceFilter').addEventListener('input', renderDevices);
    $('#settingsForm').addEventListener('submit', saveSettings);
    $('#clearApiKey').addEventListener('click', clearApiKey);
    $('#testNodes').addEventListener('click', testNodes);
    $('#logoutBtn').addEventListener('click', logout);
    $('#reloadSettings').addEventListener('click', loadSettings);

    $$('.nav-link').forEach((link) => {
      link.addEventListener('click', () => switchView(link.dataset.view));
    });
    $$('#pairingLimit button').forEach((button) => {
      button.addEventListener('click', () => {
        $$('#pairingLimit button').forEach((b) => b.classList.toggle('is-active', b === button));
        state.pairingLimit = Number(button.dataset.limit) || 100;
        loadPairings();
      });
    });

    $('#devicesBody').addEventListener('click', (event) => {
      const button = event.target.closest('[data-delete-device]');
      if (button) deleteDevice(button.dataset.deleteDevice);
    });
    $('#tokensBody').addEventListener('click', (event) => {
      const button = event.target.closest('[data-revoke-token]');
      if (button) revokeToken(button.dataset.revokeToken);
    });

    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'visible' && state.token) refreshAll();
    });
    setInterval(() => {
      if (!state.token || document.visibilityState !== 'visible' || state.view === 'settings') return;
      refreshAll();
    }, 15000);
  }

  async function boot() {
    bind();
    if (!state.token) {
      showLogin();
      return;
    }
    try {
      // Validate the stored token before hiding the login overlay.
      await api('/api/admin/stats');
      hideLogin();
      await refreshAll();
    } catch {
      showLogin('管理员令牌无效或已过期');
    }
  }

  document.addEventListener('DOMContentLoaded', boot);
})();
