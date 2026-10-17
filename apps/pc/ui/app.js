/* RustVNC desktop UI — vanilla ES2022, talks to the Tauri backend. */
(() => {
  'use strict';

  const invoke = window.__TAURI__?.core?.invoke
    ?? (async () => { throw new Error('Tauri IPC 不可用'); });
  const listen = window.__TAURI__?.event?.listen
    ?? (async () => () => {});

  const $ = (selector, root = document) => root.querySelector(selector);
  const $$ = (selector, root = document) => Array.from(root.querySelectorAll(selector));

  const state = {
    settings: null,
    version: '0.1.0',
    controllerId: '',
    agent: { running: false, online: false, clients: 0, message: '', identity: null },
    devices: [],
    paired: [],
    session: null,
    connecting: false,
    passwordVisible: false,
    lastFrameUrl: null,
    lastMainView: 'controller',
  };

  // ------------------------------------------------------------------ utils
  function escapeHtml(value) {
    return String(value ?? '')
      .replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;')
      .replaceAll('"', '&quot;').replaceAll("'", '&#39;');
  }

  function errText(error) {
    if (typeof error === 'string') return error;
    return error?.message ? String(error.message) : String(error);
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

  let toastTimer = null;
  function toast(message, isError = false) {
    const element = $('#toast');
    element.textContent = message;
    element.classList.toggle('is-error', isError);
    element.classList.add('is-visible');
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => element.classList.remove('is-visible'), 3400);
  }

  // ------------------------------------------------------------------ modal
  let modalResolve = null;

  function openModal({ title, text = '', input = false, value = '', placeholder = '', ok = '确定', cancel = '取消', danger = false }) {
    return new Promise((resolve) => {
      modalResolve = resolve;
      $('#modalTitle').textContent = title;
      $('#modalText').textContent = text;
      const field = $('#modalInput');
      field.hidden = !input;
      field.value = value;
      field.placeholder = placeholder;
      const okButton = $('#modalOk');
      okButton.textContent = ok;
      okButton.className = danger ? 'danger-btn' : 'primary-btn';
      $('#modalCancel').textContent = cancel;
      $('#modalCancel').style.display = cancel ? '' : 'none';
      $('#modal').hidden = false;
      setTimeout(() => { if (input) field.focus(); else okButton.focus(); }, 60);
    });
  }

  function closeModal(value) {
    $('#modal').hidden = true;
    const resolve = modalResolve;
    modalResolve = null;
    if (resolve) resolve(value);
  }

  // ------------------------------------------------------------------ theme
  function applyTheme(theme) {
    const normalized = theme === 'light' ? 'light' : 'dark';
    document.documentElement.dataset.theme = normalized;
    if (state.settings) state.settings.theme = normalized;
    const button = $('#themeBtn');
    if (button) {
      button.title = normalized === 'light' ? '切换为暗黑主题' : '切换为明亮主题';
    }
  }

  async function toggleTheme() {
    const next = document.documentElement.dataset.theme === 'light' ? 'dark' : 'light';
    const previous = next === 'light' ? 'dark' : 'light';
    applyTheme(next);
    try {
      await invoke('set_theme', { theme: next });
      toast(next === 'light' ? '已切换为明亮主题' : '已切换为暗黑主题');
    } catch (error) {
      applyTheme(previous);
      toast(`主题保存失败：${errText(error)}`, true);
    }
  }

  // ------------------------------------------------------------------ views
  function showView(name) {
    if (name !== 'settings' && name !== 'session') state.lastMainView = name;
    state.view = name;
    $$('.view').forEach((view) => view.classList.toggle('is-visible', view.id === `view-${name}`));
    $$('.tab[data-view]').forEach((tab) => tab.classList.toggle('is-active', tab.dataset.view === name));
    if (name === 'settings') fillSettings();
  }

  // ------------------------------------------------------------- devices/UI
  function renderDevices() {
    const list = $('#deviceList');
    list.innerHTML = state.devices.map((device, index) => `
      <div class="device-item" data-code="${escapeHtml(device.device_code)}" style="animation-delay:${Math.min(index * 45, 240)}ms">
        <span class="device-icon">
          <svg viewBox="0 0 24 24" fill="none"><rect x="2.5" y="4" width="19" height="13" rx="2.2" stroke="currentColor" stroke-width="1.6"/><path d="M8.5 20.5h7M12 17v3.5" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>
        </span>
        <div class="device-meta">
          <strong>${escapeHtml(device.device_name || '未命名设备')}</strong>
          <span class="code">${escapeHtml(device.device_code)}</span>
          <span class="sub">${device.last_connected ? `上次连接 ${escapeHtml(formatTime(device.last_connected))}` : '尚未连接'}${device.tailscale_ip ? ` · ${escapeHtml(device.tailscale_ip)}` : ''}</span>
        </div>
        <span class="status"><i class="dot ${device.online ? 'online' : 'offline'}"></i></span>
        <button class="device-remove" type="button" data-remove="${escapeHtml(device.device_code)}" title="移除">
          <svg viewBox="0 0 24 24" fill="none"><path d="M4 7h16M9.5 7V4.5h5V7M7 7l1 13h8l1-13M10 11v6M14 11v6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>
        </button>
      </div>`).join('');
    $('#devicesEmpty').hidden = state.devices.length !== 0;
  }

  function renderPaired() {
    const list = state.paired || [];
    $('#pairedCount').textContent = `${list.length} 个`;
    $('#pairedEmpty').hidden = list.length !== 0;
    $('#pairedList').innerHTML = list.map((controller, index) => `
      <div class="paired-item" style="animation-delay:${Math.min(index * 45, 240)}ms">
        <span class="device-icon">
          <svg viewBox="0 0 24 24" fill="none"><rect x="6.5" y="2.5" width="11" height="19" rx="2.4" stroke="currentColor" stroke-width="1.6"/><path d="M10.5 18.5h3" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>
        </span>
        <div class="meta">
          <strong>${escapeHtml(controller.name || '控制端')}</strong>
          <span>${escapeHtml(controller.platform || 'unknown')} · 配对于 ${escapeHtml(formatTime(controller.paired_at))}</span>
        </div>
        <button class="unpair-btn" type="button" data-token="${escapeHtml(controller.token_id)}">解除配对</button>
      </div>`).join('');
  }

  function renderAgent() {
    const identity = state.agent.identity;
    const running = state.agent.running;
    $('#agentEmpty').hidden = !!identity;
    $('#agentPanel').hidden = !identity;
    if (!identity) return;

    $('#agentCode').textContent = identity.device_code;
    $('#agentPassword').textContent = state.passwordVisible ? identity.password : '••••••••';
    $('#togglePassword').textContent = state.passwordVisible ? '隐藏密码' : '显示密码';

    const dot = $('#agentStatus .dot');
    dot.className = `dot ${running ? (state.agent.online ? 'online' : 'warn') : 'offline'}`;
    $('#agentStatusText').textContent = running ? (state.agent.online ? '在线' : '连接中') : '未启动';

    const toggle = $('#agentStartStop');
    toggle.textContent = running ? '停止服务' : '启动服务';
    toggle.className = running ? 'danger-btn' : 'primary-btn';

    $('#agentIp').textContent = identity.tailscale_ip || '待分配';
    $('#agentPort').textContent = state.settings?.listen_port ?? '—';
    $('#agentClients').textContent = state.agent.clients ?? 0;
    $('#agentNetwork').textContent = state.agent.message || '—';

    const tag = $('#agentHeadscaleTag');
    tag.textContent = identity.headscale_configured ? 'Headscale 已配置' : 'Headscale 未配置';
    tag.className = `tag ${identity.headscale_configured ? 'ok' : 'warn'}`;
    $('#agentHint').textContent = running
      ? (state.agent.message || '服务运行中')
      : '服务未启动，设备码仍可查看。';
    renderPaired();
  }

  function fillSettings() {
    const settings = state.settings;
    if (!settings) return;
    $('#setServerUrl').value = settings.server_url || '';
    $('#setDeviceName').value = settings.device_name || '';
    $('#setControllerName').value = settings.controller_name || '';
    $('#setNetworkMode').value = settings.network_mode || 'direct';
    $('#setBind').value = settings.listen_bind || '127.0.0.1';
    $('#setPort').value = settings.listen_port || 21416;
    $('#setTailscaleIp').value = settings.tailscale_ip || '';
    $('#setFps').value = settings.fps || 15;
    $('#setQuality').value = settings.quality || 70;
    $('#setMaxWidth').value = settings.max_width ?? 0;
    $('#aboutVersion').textContent = state.version;
    $('#aboutControllerId').textContent = state.controllerId || settings.controller_id || '—';
    $('#pairServerTag').textContent = settings.server_url || '配对服务器';
  }

  function renderAll() {
    renderDevices();
    renderAgent();
    fillSettings();
  }

  // ------------------------------------------------------------------ pairing
  const codeInputs = () => $$('.code-digit');

  function getCode() {
    return codeInputs().map((input) => input.value).join('');
  }

  function clearCodeInputs() {
    codeInputs().forEach((input) => { input.value = ''; input.classList.remove('filled'); });
    codeInputs()[0]?.focus();
  }

  function fillCode(code) {
    const chars = String(code).toUpperCase().replace(/[^0-9A-Z]/g, '').slice(0, 6).split('');
    codeInputs().forEach((input, index) => {
      input.value = chars[index] || '';
      input.classList.toggle('filled', Boolean(chars[index]));
    });
    const next = Math.min(chars.length, 5);
    codeInputs()[next]?.focus();
  }

  function bindCodeInputs() {
    const inputs = codeInputs();
    inputs.forEach((input, index) => {
      input.addEventListener('input', () => {
        const cleaned = input.value.toUpperCase().replace(/[^0-9A-Z]/g, '');
        input.value = cleaned.slice(-1);
        if (input.value) {
          input.classList.add('filled');
          setTimeout(() => input.classList.remove('filled'), 200);
          inputs[Math.min(index + 1, inputs.length - 1)]?.focus();
        } else {
          input.classList.remove('filled');
        }
      });
      input.addEventListener('keydown', (event) => {
        if (event.key === 'Backspace' && !input.value && index > 0) {
          inputs[index - 1].value = '';
          inputs[index - 1].focus();
          event.preventDefault();
        } else if (event.key === 'ArrowLeft' && index > 0) {
          inputs[index - 1].focus();
          event.preventDefault();
        } else if (event.key === 'ArrowRight' && index < inputs.length - 1) {
          inputs[index + 1].focus();
          event.preventDefault();
        } else if (event.key === 'Enter') {
          $('#pairForm').requestSubmit();
        }
      });
      input.addEventListener('paste', (event) => {
        const text = event.clipboardData?.getData('text') || '';
        if (text) {
          event.preventDefault();
          fillCode(text);
        }
      });
    });
  }

  async function submitPair(event) {
    event.preventDefault();
    const code = getCode();
    const password = $('#pairPassword').value;
    if (code.length !== 6) return toast('请输入完整的 6 位设备码', true);
    if (!password) return toast('请输入被控端显示的密码', true);

    const button = $('#pairBtn');
    setBusy(button, true);
    try {
      const view = await invoke('pair_device', { code, password });
      state.devices = upsertDevice(state.devices, view);
      renderDevices();
      clearCodeInputs();
      $('#pairPassword').value = '';
      toast(`配对成功：${view.device_name}`);
      await connectDevice(view.device_code);
    } catch (error) {
      toast(errText(error), true);
    } finally {
      setBusy(button, false);
    }
  }

  function setBusy(button, busy) {
    button.disabled = busy;
    const label = $('.btn-label', button);
    const spinner = $('.spinner', button);
    if (label) label.textContent = busy ? '正在配对…' : '配对并连接';
    if (spinner) spinner.hidden = !busy;
  }

  function upsertDevice(devices, view) {
    const next = devices.filter((device) => device.device_code !== view.device_code);
    next.unshift(view);
    return next;
  }

  // ------------------------------------------------------------------ session
  async function connectDevice(code) {
    if (state.connecting) return;
    state.connecting = true;
    showSessionConnecting(code);
    try {
      const info = await invoke('connect_device', { code });
      startSession(info);
    } catch (error) {
      hideSession();
      toast(errText(error), true);
      state.connecting = false;
    }
  }

  function showSessionConnecting(code) {
    const device = state.devices.find((entry) => entry.device_code === code);
    $('#sessionView').hidden = false;
    $('#sessionPlaceholder').hidden = false;
    $('#sessionImage').classList.remove('is-live');
    $('#sessionDevice').textContent = device?.device_name || code;
    $('#sessionDetail').textContent = '正在建立加密会话…';
    $('#sessionView').classList.remove('idle', 'hint-gone');
    bumpSessionUi();
  }

  function startSession(info) {
    if (state.session?.info?.session_id === info.session_id) {
      state.session.info = info;
      return;
    }
    state.session = { info };
    state.connecting = false;
    $('#sessionView').hidden = false;
    $('#sessionPlaceholder').hidden = false;
    $('#sessionDevice').textContent = `${info.device_name} · ${info.device_code}`;
    $('#sessionDetail').textContent = `${info.width}×${info.height} · ${info.fps} fps · ${info.transport}`;
    $('#sessionView').classList.remove('idle', 'hint-gone');
    bumpSessionUi();
    setTimeout(() => $('#sessionHint').style.display = 'none', 3800);
    setTimeout(() => $('#sessionIme').focus({ preventScroll: true }), 80);
    toast('已连接，点击画面后即可使用键盘');
  }

  function hideSession() {
    $('#sessionView').hidden = true;
    state.session = null;
    state.connecting = false;
    releaseAllKeys();
    const image = $('#sessionImage');
    image.classList.remove('is-live');
    image.removeAttribute('src');
    if (state.lastFrameUrl) {
      URL.revokeObjectURL(state.lastFrameUrl);
      state.lastFrameUrl = null;
    }
    $('#sessionHint').style.display = '';
  }

  function endSession(reason) {
    const wasActive = Boolean(state.session) || state.connecting;
    hideSession();
    if (wasActive) {
      toast(reason ? `会话已断开：${reason}` : '会话已断开', Boolean(reason));
    }
  }

  // ------------------------------------------------------------------ frames
  let pendingFrame = null;
  let frameScheduled = false;

  function queueFrame(payload) {
    if (!state.session) return;
    pendingFrame = payload;
    if (!frameScheduled) {
      frameScheduled = true;
      requestAnimationFrame(applyFrame);
    }
  }

  function applyFrame() {
    frameScheduled = false;
    const payload = pendingFrame;
    pendingFrame = null;
    if (!payload || !state.session) return;
    try {
      const binary = atob(payload.data_b64);
      const bytes = new Uint8Array(binary.length);
      for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
      const url = URL.createObjectURL(new Blob([bytes], { type: 'image/jpeg' }));
      const image = $('#sessionImage');
      image.src = url;
      if (state.lastFrameUrl) URL.revokeObjectURL(state.lastFrameUrl);
      state.lastFrameUrl = url;
      image.classList.add('is-live');
      $('#sessionPlaceholder').hidden = true;
    } catch {
      /* ignore malformed frames */
    }
  }

  // ---------------------------------------------------------------- input map
  const pressedKeys = new Set();
  const activeButtons = new Set();
  let composing = false;

  function isEditable(target) {
    return target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable);
  }

  function sendInput(event) {
    invoke('send_input', { event }).catch(() => {});
  }

  function contentRect() {
    const info = state.session?.info;
    if (!info) return null;
    const stage = $('#sessionStage');
    const rect = stage.getBoundingClientRect();
    const scale = Math.min(rect.width / info.width, rect.height / info.height);
    const width = info.width * scale;
    const height = info.height * scale;
    return {
      rect,
      width,
      height,
      left: rect.left + (rect.width - width) / 2,
      top: rect.top + (rect.height - height) / 2,
      scale,
    };
  }

  function toNormalized(event) {
    const content = contentRect();
    if (!content) return null;
    const x = (event.clientX - content.left) / content.width;
    const y = (event.clientY - content.top) / content.height;
    if (x < 0 || x > 1 || y < 0 || y > 1) return null;
    return { x, y };
  }

  function buttonName(button) {
    return ['Left', 'Middle', 'Right', 'Back', 'Forward'][button] || null;
  }

  function isToolbarTarget(event) {
    return Boolean(event.target.closest?.('.session-toolbar'));
  }

  let movePending = null;
  let moveScheduled = false;

  function queueMove(event) {
    movePending = { clientX: event.clientX, clientY: event.clientY };
    if (moveScheduled) return;
    moveScheduled = true;
    requestAnimationFrame(() => {
      moveScheduled = false;
      const pending = movePending;
      movePending = null;
      if (!pending || !state.session) return;
      const normalized = toNormalized(pending);
      if (normalized) sendInput({ MouseMove: normalized });
    });
  }

  function bindSessionInput() {
    const stage = $('#sessionStage');
    const view = $('#sessionView');

    stage.addEventListener('pointermove', (event) => {
      if (!state.session || isToolbarTarget(event)) return;
      queueMove(event);
      bumpSessionUi();
    });
    stage.addEventListener('pointerdown', (event) => {
      if (!state.session || isToolbarTarget(event)) return;
      $('#sessionIme').focus({ preventScroll: true });
      const normalized = toNormalized(event);
      if (normalized) sendInput({ MouseMove: normalized });
      const name = buttonName(event.button);
      if (name) {
        activeButtons.add(event.button);
        sendInput({ MouseButton: { button: name, pressed: true } });
      }
      event.preventDefault();
    });
    window.addEventListener('pointerup', (event) => {
      if (!state.session) return;
      if (!activeButtons.has(event.button)) return;
      activeButtons.delete(event.button);
      const name = buttonName(event.button);
      if (name) sendInput({ MouseButton: { button: name, pressed: false } });
    });
    stage.addEventListener('wheel', (event) => {
      if (!state.session || isToolbarTarget(event)) return;
      event.preventDefault();
      sendInput({
        Wheel: { delta_x: event.deltaX / 100, delta_y: event.deltaY / 100 },
      });
    }, { passive: false });
    stage.addEventListener('contextmenu', (event) => event.preventDefault());

    window.addEventListener('keydown', (event) => {
      if (!state.session) return;
      if (isEditable(event.target) && event.target.id !== 'sessionIme') return;
      if (event.isComposing) return;
      if (event.key === 'F11') {
        event.preventDefault();
        invoke('toggle_fullscreen').catch(() => {});
        return;
      }
      if (!event.code) return;
      event.preventDefault();
      if (event.repeat) return;
      if (event.key.length === 1 && event.key.charCodeAt(0) > 127) {
        invoke('send_text', { text: event.key }).catch(() => {});
        return;
      }
      pressedKeys.add(event.code);
      sendInput({ Key: { code: event.code, pressed: true } });
    });
    window.addEventListener('keyup', (event) => {
      if (!state.session) return;
      if (isEditable(event.target) && event.target.id !== 'sessionIme') return;
      if (!event.code || !pressedKeys.has(event.code)) return;
      pressedKeys.delete(event.code);
      event.preventDefault();
      sendInput({ Key: { code: event.code, pressed: false } });
    });
    window.addEventListener('blur', () => {
      if (state.session) releaseAllKeys();
      view.classList.add('idle');
    });

    const ime = $('#sessionIme');
    ime.addEventListener('compositionstart', () => { composing = true; });
    ime.addEventListener('compositionend', (event) => {
      composing = false;
      const text = event.data;
      ime.value = '';
      if (text && state.session) invoke('send_text', { text }).catch(() => {});
    });
    ime.addEventListener('input', () => {
      if (!composing) ime.value = '';
    });
  }

  function releaseAllKeys() {
    for (const code of pressedKeys) {
      invoke('send_input', { event: { Key: { code, pressed: false } } }).catch(() => {});
    }
    pressedKeys.clear();
    for (const button of activeButtons) {
      const name = buttonName(button);
      if (name) {
        invoke('send_input', { event: { MouseButton: { button: name, pressed: false } } }).catch(() => {});
      }
    }
    activeButtons.clear();
  }

  let idleTimer = null;
  function bumpSessionUi() {
    const view = $('#sessionView');
    view.classList.remove('idle');
    clearTimeout(idleTimer);
    idleTimer = setTimeout(() => view.classList.add('idle'), 2600);
  }

  // ------------------------------------------------------------------- events
  function subscribe() {
    listen('agent://status', (event) => {
      const payload = event.payload;
      state.agent.online = payload.online;
      state.agent.clients = payload.clients;
      state.agent.message = payload.message;
      renderAgent();
    });
    listen('agent://paired', (event) => {
      state.paired = event.payload?.controllers || [];
      renderPaired();
    });
    listen('agent://identity', (event) => {
      state.agent.identity = event.payload;
      if (!event.payload) state.agent.running = false;
      renderAgent();
    });
    listen('agent://session', (event) => {
      const payload = event.payload;
      if (payload.state === 'started') {
        toast(`控制端「${payload.controller}」已接入`);
      } else {
        toast(`控制端「${payload.controller}」已断开${payload.reason ? `：${payload.reason}` : ''}`);
      }
    });
    listen('session://frame', (event) => queueFrame(event.payload));
    listen('session://notice', (event) => toast(event.payload?.message || ''));
    listen('session://state', (event) => {
      const payload = event.payload;
      if (payload.state === 'connected' && payload.info) {
        startSession(payload.info);
      } else if (payload.state === 'disconnected') {
        endSession(payload.reason || '');
      }
    });
    listen('devices://updated', (event) => {
      const payload = event.payload;
      if (Array.isArray(payload)) state.devices = payload;
      else if (payload?.device_code) state.devices = upsertDevice(state.devices, payload);
      renderDevices();
    });
    listen('settings://updated', (event) => {
      state.settings = event.payload;
      applyTheme(state.settings?.theme || 'dark');
      fillSettings();
    });
  }

  // ------------------------------------------------------------------ actions
  async function toggleAgent() {
    const button = $('#agentStartStop');
    const wasRunning = state.agent.running;
    button.disabled = true;
    button.textContent = wasRunning ? '正在停止…' : '正在启动…';
    try {
      const snapshot = wasRunning
        ? await invoke('stop_agent')
        : await invoke('start_agent');
      state.agent = { ...state.agent, ...snapshot };
      if (!snapshot.running) state.paired = [];
      renderAgent();
      toast(wasRunning ? '被控服务已停止' : '被控服务已启动');
    } catch (error) {
      toast(errText(error), true);
      renderAgent();
    } finally {
      button.disabled = false;
    }
  }

  async function rotatePassword() {
    if (!state.agent.identity) return;
    const value = await openModal({
      title: '修改配对密码',
      text: '输入 8–64 位新密码；留空则由服务器生成随机密码。修改后，新的配对需要使用新密码。',
      input: true,
      placeholder: '留空自动生成',
      ok: '修改密码',
      cancel: '取消',
    });
    if (value === null) return;
    try {
      const password = await invoke('rotate_agent_password', {
        newPassword: value.trim() ? value.trim() : null,
      });
      state.passwordVisible = true;
      if (state.agent.identity) state.agent.identity.password = password;
      renderAgent();
      toast('密码已更新');
    } catch (error) {
      toast(errText(error), true);
    }
  }

  async function forgetAgent() {
    const confirmed = await openModal({
      title: '重新注册被控端',
      text: '将停止服务并删除本机保存的设备码。下次启动会注册一个新设备码，旧设备码将失效。此操作不影响已配对的令牌记录（可在服务器后台删除设备）。',
      ok: '重新注册',
      cancel: '取消',
      danger: true,
    });
    if (confirmed === null) return;
    try {
      const snapshot = await invoke('forget_agent');
      state.agent = { ...state.agent, ...snapshot, identity: null };
      state.paired = [];
      renderAgent();
      toast('已清除本机设备身份');
    } catch (error) {
      toast(errText(error), true);
    }
  }

  async function revokeController(tokenId) {
    const confirmed = await openModal({
      title: '解除配对',
      text: '解除后，该控制端下次连接需要重新输入设备码和密码。',
      ok: '解除配对',
      cancel: '取消',
      danger: true,
    });
    if (confirmed === null) return;
    try {
      await invoke('revoke_controller', { tokenId });
      state.paired = state.paired.filter((entry) => entry.token_id !== tokenId);
      renderPaired();
      toast('已解除配对');
    } catch (error) {
      toast(errText(error), true);
    }
  }

  async function removeDevice(code) {
    const confirmed = await openModal({
      title: '移除设备',
      text: `删除本机保存的 ${code} 免密令牌？之后需要重新配对。`,
      ok: '移除',
      cancel: '取消',
      danger: true,
    });
    if (confirmed === null) return;
    try {
      const devices = await invoke('remove_saved_device', { code });
      state.devices = devices;
      renderDevices();
      toast('已移除');
    } catch (error) {
      toast(errText(error), true);
    }
  }

  async function enrollNetwork() {
    try {
      const result = await invoke('enroll_network');
      const text = [
        `控制面：${result.control_url}`,
        `主机名：${result.hostname}`,
        '',
        '将下面的密钥填入 tsnet sidecar 的 TS_AUTHKEY 环境变量。',
        '密钥只显示一次，请立即保存。',
      ].join('\n');
      const value = await openModal({
        title: '网络接入密钥',
        text,
        input: true,
        value: result.preauth_key,
        ok: '复制密钥',
        cancel: '关闭',
      });
      if (value !== null) {
        try {
          await navigator.clipboard.writeText(result.preauth_key);
          toast('密钥已复制');
        } catch {
          toast('请手动复制密钥', true);
        }
      }
    } catch (error) {
      toast(errText(error), true);
    }
  }

  async function saveSettings(event) {
    event.preventDefault();
    if (!state.settings) return;
    const button = $('#saveSettingsBtn');
    button.disabled = true;
    const payload = {
      ...state.settings,
      server_url: $('#setServerUrl').value.trim(),
      device_name: $('#setDeviceName').value.trim(),
      controller_name: $('#setControllerName').value.trim(),
      network_mode: $('#setNetworkMode').value.trim(),
      listen_bind: $('#setBind').value.trim(),
      listen_port: Number($('#setPort').value) || 0,
      tailscale_ip: $('#setTailscaleIp').value.trim(),
      fps: Number($('#setFps').value) || 0,
      quality: Number($('#setQuality').value) || 0,
      max_width: Number($('#setMaxWidth').value) || 0,
    };
    try {
      state.settings = await invoke('save_settings', { settings: payload });
      applyTheme(state.settings.theme);
      fillSettings();
      toast('设置已保存');
      if (state.agent.running) toast('网络与画面设置需重启被控服务后生效');
    } catch (error) {
      toast(errText(error), true);
    } finally {
      button.disabled = false;
    }
  }

  async function openSessionText() {
    if (!state.session) return;
    const text = await openModal({
      title: '发送文本',
      text: '将文本作为 Unicode 输入发送到被控端，适合中文或特殊字符。',
      input: true,
      placeholder: '输入要发送的文本',
      ok: '发送',
      cancel: '取消',
    });
    if (!text) return;
    try {
      await invoke('send_text', { text });
      toast('文本已发送');
    } catch (error) {
      toast(errText(error), true);
    }
  }

  async function disconnectSession() {
    try {
      await invoke('disconnect_session');
    } catch (error) {
      toast(errText(error), true);
    }
    endSession('已断开');
  }

  async function sessionSettings() {
    if (!state.session) return;
    const result = await openModal({
      title: '会话设置',
      text: '会话画面始终按原宽高比等比显示，不会拉伸。可在此切换全屏或断开连接。',
      input: false,
      ok: '全屏 / 窗口',
      cancel: '关闭',
    });
    if (result !== null) {
      try {
        const fullscreen = await invoke('toggle_fullscreen');
        toast(fullscreen ? '已进入全屏' : '已退出全屏');
      } catch (error) {
        toast(errText(error), true);
      }
    }
  }

  // -------------------------------------------------------------------- bind
  function bind() {
    $('#tabController').addEventListener('click', () => showView('controller'));
    $('#tabAgent').addEventListener('click', () => showView('agent'));
    $('#themeBtn').addEventListener('click', toggleTheme);
    $('#settingsBtn').addEventListener('click', () => showView('settings'));
    $('#backFromSettings').addEventListener('click', () => showView(state.lastMainView || 'controller'));
    $('#reloadSettingsBtn').addEventListener('click', fillSettings);
    $('#settingsForm').addEventListener('submit', saveSettings);
    $('#enrollNetwork').addEventListener('click', enrollNetwork);

    $('#winMin').addEventListener('click', () => invoke('window_minimize').catch(() => {}));
    $('#winMax').addEventListener('click', () => invoke('window_toggle_maximize').catch(() => {}));
    $('#winClose').addEventListener('click', () => invoke('window_close').catch(() => {}));

    $('#pairForm').addEventListener('submit', submitPair);
    bindCodeInputs();

    $('#togglePairPassword').addEventListener('click', () => {
      const input = $('#pairPassword');
      input.type = input.type === 'password' ? 'text' : 'password';
    });

    $('#refreshDevices').addEventListener('click', async () => {
      try {
        state.devices = await invoke('list_saved_devices');
        renderDevices();
        toast('设备列表已刷新');
      } catch (error) {
        toast(errText(error), true);
      }
    });

    $('#deviceList').addEventListener('click', (event) => {
      const remove = event.target.closest('[data-remove]');
      if (remove) {
        event.stopPropagation();
        removeDevice(remove.dataset.remove);
        return;
      }
      const item = event.target.closest('[data-code]');
      if (item) connectDevice(item.dataset.code);
    });

    $('#agentStartStop').addEventListener('click', toggleAgent);
    $('#startAgentBtn').addEventListener('click', toggleAgent);
    $('#copyCode').addEventListener('click', async () => {
      const code = state.agent.identity?.device_code;
      if (!code) return;
      try {
        await navigator.clipboard.writeText(code);
        toast('设备码已复制');
      } catch {
        toast(`设备码：${code}`, false);
      }
    });
    $('#togglePassword').addEventListener('click', () => {
      state.passwordVisible = !state.passwordVisible;
      renderAgent();
    });
    $('#rotatePassword').addEventListener('click', rotatePassword);
    $('#forgetAgent').addEventListener('click', forgetAgent);
    $('#pairedList').addEventListener('click', (event) => {
      const button = event.target.closest('[data-token]');
      if (button) revokeController(button.dataset.token);
    });

    $('#sessionText').addEventListener('click', openSessionText);
    $('#sessionFullscreen').addEventListener('click', async () => {
      try {
        const fullscreen = await invoke('toggle_fullscreen');
        toast(fullscreen ? '已进入全屏' : '已退出全屏');
      } catch (error) {
        toast(errText(error), true);
      }
    });
    $('#sessionDisconnect').addEventListener('click', disconnectSession);
    $('#sessionSettings').addEventListener('click', sessionSettings);

    $('#modalOk').addEventListener('click', () => {
      const field = $('#modalInput');
      closeModal(field.hidden ? '' : field.value);
    });
    $('#modalCancel').addEventListener('click', () => closeModal(null));
    $('#modal').addEventListener('click', (event) => {
      if (event.target === $('#modal')) closeModal(null);
    });
    $('#modalInput').addEventListener('keydown', (event) => {
      if (event.key === 'Enter') {
        event.preventDefault();
        closeModal($('#modalInput').value);
      }
    });
    document.addEventListener('keydown', (event) => {
      if (event.key === 'Escape' && !$('#modal').hidden) {
        closeModal(null);
      }
    });

    bindSessionInput();
  }

  async function boot() {
    bind();
    subscribe();
    try {
      const payload = await invoke('bootstrap');
      state.settings = payload.settings;
      state.agent = payload.agent;
      state.devices = payload.saved_devices;
      state.version = payload.version;
      state.controllerId = payload.controller_id;
      applyTheme(payload.settings?.theme || 'dark');
      if (!state.agent.identity && state.settings?.device_name) {
        // keep the activation screen informative
      }
    } catch (error) {
      toast(`初始化失败：${errText(error)}`, true);
    }
    renderAll();
    setTimeout(() => showView('controller'), 0);
  }

  document.addEventListener('DOMContentLoaded', boot);
})();
