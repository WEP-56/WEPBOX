const CORE_RESTART_KEYS = new Set([
  'allowLan',
  'dnsGuardEnabled',
  'ipv6Enabled',
  'fakeDnsEnabled',
  'tunAutoRoute',
  'tunStrictRoute'
]);

const PENDING_SETTING_KEYS = new Set([
]);

const systemThemeQuery = window.matchMedia?.('(prefers-color-scheme: light)');

function applyThemePreference(current = settings){
  const useLightTheme = Boolean(current?.followSystemTheme && systemThemeQuery?.matches);
  document.documentElement.classList.toggle('light-theme', useLightTheme);
}

if(systemThemeQuery?.addEventListener){
  systemThemeQuery.addEventListener('change', () => applyThemePreference(settings));
} else if(systemThemeQuery?.addListener){
  systemThemeQuery.addListener(() => applyThemePreference(settings));
}

function scrollToSettingsSection(sectionId){
  document.getElementById(`settings-section-${sectionId}`)?.scrollIntoView({
    behavior: 'smooth',
    block: 'start'
  });
}

async function toggleTun(){
  if(!settings){
    renderTun(!document.getElementById('tun-mode-btn').classList.contains('on'));
    return;
  }

  const nextValue = !settings.tunEnabled;
  if(nextValue && invoke){
    const isAdmin = await invoke('check_admin').catch(() => false);
    if(!isAdmin){
      appendLog('[INFO] requesting administrator elevation for TUN mode');
      showToast('启用 TUN 需要管理员权限，正在请求 Windows 提权确认。');
      await invoke('restart_as_admin', {
        enableTun: true,
        resumeProxy: Boolean(status?.coreRunning)
      });
      return;
    }
  }

  settings = normalizeSettings(settings);
  settings.tunEnabled = nextValue;
  renderTun(settings.tunEnabled);
  renderSettingsPanel(settings);
  await saveCurrentSettings({ restartCore: true });
  appendLog(`[INFO] tun mode preference changed: ${settings.tunEnabled ? 'enabled' : 'disabled'}`);
}

async function setMode(el, mode){
  document.querySelectorAll('[data-mode]').forEach(button => button.classList.remove('on'));
  el.classList.add('on');
  settings = normalizeSettings(settings);
  settings.mode = mode;
  renderSettingsPanel(settings);
  await saveCurrentSettings({ restartCore: true });
  appendLog(`[INFO] route mode saved: ${mode}`);
}

async function toggleSetting(key, options = {}){
  settings = normalizeSettings(settings);
  settings[key] = !settings[key];
  renderSettingsPanel(settings);
  await saveCurrentSettings({
    restartCore: options.restartCore ?? CORE_RESTART_KEYS.has(key),
    pendingOnly: options.pendingOnly ?? PENDING_SETTING_KEYS.has(key)
  });
}

async function updateSelectSetting(key, value, options = {}){
  settings = normalizeSettings(settings);
  settings[key] = value;
  renderSettingsPanel(settings);
  await saveCurrentSettings({
    restartCore: Boolean(options.restartCore),
    pendingOnly: options.pendingOnly ?? PENDING_SETTING_KEYS.has(key)
  });
}

async function updateNumericSetting(key, value, options = {}){
  settings = normalizeSettings(settings);
  const parsed = Number.parseInt(value, 10);
  if(!Number.isFinite(parsed) || parsed < 0){
    renderSettingsPanel(settings);
    showToast(options.invalidMessage || '请输入有效数字。');
    return;
  }
  settings[key] = parsed;
  renderSettingsPanel(settings);
  await saveCurrentSettings({
    restartCore: Boolean(options.restartCore),
    pendingOnly: options.pendingOnly ?? PENDING_SETTING_KEYS.has(key)
  });
}

async function updatePortSetting(key, value){
  settings = normalizeSettings(settings);
  const parsed = Number.parseInt(value, 10);
  if(!Number.isFinite(parsed) || parsed <= 0 || parsed > 65535){
    renderSettingsPanel(settings);
    showToast('请输入 1 到 65535 之间的有效端口号。');
    return;
  }
  settings[key] = parsed;
  renderSettingsPanel(settings);
  await saveCurrentSettings({ restartCore: true });
}

async function updateTextSetting(key, value, options = {}){
  settings = normalizeSettings(settings);
  const trimmed = String(value || '').trim();
  settings[key] = options.nullable !== false
    ? (trimmed || null)
    : trimmed;
  renderSettingsPanel(settings);
  await saveCurrentSettings({
    restartCore: Boolean(options.restartCore),
    pendingOnly: Boolean(options.pendingOnly)
  });
}

async function updateCustomDnsServers(value){
  settings = normalizeSettings(settings);
  settings.customDnsServers = splitTextareaLines(value);
  renderSettingsPanel(settings);
  await saveCurrentSettings({ restartCore: true });
}

async function updateRouteExcludeAddresses(value){
  settings = normalizeSettings(settings);
  settings.tunRouteExcludeAddress = splitTextareaLines(value);
  renderSettingsPanel(settings);
  await saveCurrentSettings({ restartCore: true });
}

function splitTextareaLines(value){
  return String(value || '')
    .split(/\r?\n|,/)
    .map(item => item.trim())
    .filter(Boolean);
}

async function refreshMaintenanceInfo(){
  if(!invoke){
    maintenanceInfo = null;
    renderMaintenanceInfo();
    return;
  }
  try{
    maintenanceInfo = await invoke('maintenance_info');
    renderMaintenanceInfo();
  }catch(err){
    appendLog('[WARN] failed to load maintenance info: ' + formatError(err));
  }
}

function renderSettingsPanel(current){
  if(!current) return;

  applyThemePreference(current);

  setToggleState('auto-launch-tog', current.autoLaunch);
  setToggleState('auto-start-proxy-tog', current.autoStartProxy);
  setToggleState('start-hidden-tog', current.startHidden);
  setToggleState('hide-to-tray-tog', current.hideToTray);
  setToggleState('notify-on-failure-tog', current.notifyOnFailure);
  setToggleState('follow-system-theme-tog', current.followSystemTheme);
  setToggleState('allow-lan-tog', current.allowLan);
  setToggleState('tun-mode-setting-tog', current.tunEnabled);
  setToggleState('dns-guard-tog', current.dnsGuardEnabled);
  setToggleState('ipv6-tog', current.ipv6Enabled);
  setToggleState('fake-dns-tog', current.fakeDnsEnabled);
  setToggleState('auto-select-fastest-tog', current.autoSelectFastest);
  setToggleState('auto-switch-on-failure-tog', current.autoSwitchOnFailure);
  setToggleState('udp-acceleration-tog', current.udpAccelerationEnabled);
  setToggleState('experimental-quic-tog', current.experimentalQuic);
  setToggleState('tun-auto-route-tog', current.tunAutoRoute);
  setToggleState('tun-strict-route-tog', current.tunStrictRoute);

  setValue('fallback-select', current.fallback || 'direct');
  setValue('auto-update-hours', String(current.autoUpdateHours ?? 24));
  setValue('speed-test-interval', current.speedTestInterval || 'every1Hour');
  setValue('local-mixed-port-input', String(current.localMixedPort ?? 7890));
  setValue('clash-api-port-input', String(current.clashApiPort ?? 9090));
  setValue('clash-api-secret-input', current.clashApiSecret || '');
  setValue('converter-url-input', current.converterUrl || '');
  setValue('dns-servers-input', (current.customDnsServers || []).join('\n'));
  setValue('fake-ipv4-input', current.fakeIpV4Range || '');
  setValue('fake-ipv6-input', current.fakeIpV6Range || '');
  setValue('tun-interface-name-input', current.tunInterfaceName || '');
  setValue('tun-mtu-input', String(current.tunMtu ?? 1500));
  setValue('tun-route-exclude-input', (current.tunRouteExcludeAddress || []).join('\n'));

  renderMaintenanceInfo();
}

function renderMaintenanceInfo(){
  const version = document.getElementById('maintenance-sidecar-version');
  const appData = document.getElementById('maintenance-app-data-path');
  const settingsPath = document.getElementById('maintenance-settings-path');
  const configPath = document.getElementById('maintenance-config-path');
  const logPath = document.getElementById('maintenance-log-path');
  const runtimePath = document.getElementById('maintenance-runtime-path');
  const subscriptionPath = document.getElementById('maintenance-subscription-path');

  if(version) version.textContent = maintenanceInfo?.sidecarVersion || '未检测到';
  if(appData) appData.textContent = maintenanceInfo?.appDataDir || '等待后端连接';
  if(settingsPath) settingsPath.textContent = maintenanceInfo?.settingsPath || '等待后端连接';
  if(configPath) configPath.textContent = maintenanceInfo?.configPath || '等待后端连接';
  if(logPath) logPath.textContent = maintenanceInfo?.logPath || '等待后端连接';
  if(runtimePath) runtimePath.textContent = maintenanceInfo?.runtimeMarkerPath || '等待后端连接';
  if(subscriptionPath) subscriptionPath.textContent = maintenanceInfo?.subscriptionsDir || '等待后端连接';
}

function setValue(id, value){
  const element = document.getElementById(id);
  if(element) element.value = value;
}

function setToggleState(id, enabled){
  document.getElementById(id)?.classList.toggle('on', Boolean(enabled));
}

async function runMaintenanceAction(command, successMessage){
  if(!invoke){
    showToast('当前不在 Tauri 环境中，无法执行维护操作。');
    return;
  }

  try{
    const result = await invoke(command);
    if(successMessage){
      showToast(successMessage);
    } else if(result?.message){
      showToast(result.message);
    }
    if(result?.message){
      appendLog(`[INFO] ${result.message}${result.path ? `: ${result.path}` : ''}`);
    }
    await refreshMaintenanceInfo();
    return result;
  }catch(err){
    const message = formatError(err);
    showToast(message);
    appendLog('[ERROR] ' + message);
    throw err;
  }
}

async function openAppDataDir(){
  await runMaintenanceAction('open_app_data_dir');
}

async function openLogDir(){
  await runMaintenanceAction('open_log_dir');
}

async function openSettingsFile(){
  await runMaintenanceAction('open_settings_file');
}

async function openConfigFile(){
  await runMaintenanceAction('open_config_file');
}

async function openSubscriptionsDir(){
  await runMaintenanceAction('open_subscriptions_dir');
}

async function clearSingboxLog(){
  await runMaintenanceAction('clear_singbox_log', 'sing-box 日志已清理。');
}

async function refreshAllRemoteSubscriptions(){
  if(!invoke){
    showToast('当前不在 Tauri 环境中，无法刷新订阅。');
    return;
  }
  try{
    const result = await invoke('refresh_all_remote_subscriptions');
    settings = normalizeSettings(await invoke('get_settings'));
    status = await invoke('app_status');
    renderStatus(status);
    renderSubscriptions(settings.subscriptions || []);
    renderSelectedSubscriptionSummary();
    renderSettingsPanel(settings);
    if(status.coreRunning) await refreshProxies();
    showToast(`远程订阅刷新完成：更新 ${result.refreshed} 个，失败 ${result.failed} 个。`);
    appendLog(`[INFO] remote subscriptions refreshed: checked=${result.checked}, refreshed=${result.refreshed}, failed=${result.failed}, skipped=${result.skipped}, restarted=${result.restarted}`);
    if(Array.isArray(result.failures)){
      result.failures.forEach(item => appendLog(`[WARN] ${item}`));
    }
  }catch(err){
    const message = formatError(err);
    showToast(message);
    appendLog('[ERROR] ' + message);
  }
}

async function clearSubscriptionCache(){
  if(!window.confirm('清理订阅缓存会删除已导入订阅和本地缓存，并重写当前配置。继续？')) return;
  await runMaintenanceAction('clear_subscription_cache');
  if(invoke){
    settings = normalizeSettings(await invoke('get_settings'));
    status = await invoke('app_status');
    renderStatus(status);
    renderSubscriptions(settings.subscriptions || []);
    renderSelectedSubscriptionSummary();
    renderSettingsPanel(settings);
    if(status.coreRunning) await refreshProxies();
  }
}

async function runScheduledSpeedTest(){
  if(!invoke){
    showToast('当前不在 Tauri 环境中，无法执行测速计划。');
    return;
  }
  try{
    const result = await invoke('run_scheduled_speed_test');
    applySpeedTestResults(result.results || []);
    if(status?.coreRunning) await refreshProxies();
    showToast(`测速完成：成功 ${result.succeeded} 个，失败 ${result.failed} 个，自动切换 ${result.selected?.length || 0} 个分组。`);
    appendLog(`[INFO] speed test completed: tested=${result.tested}, succeeded=${result.succeeded}, failed=${result.failed}, selected=${result.selected?.length || 0}`);
    if(Array.isArray(result.selected)){
      result.selected.forEach(item => appendLog(`[INFO] selected fastest node: ${item.group} -> ${item.name} (${item.delay}ms)`));
    }
  }catch(err){
    const message = formatError(err);
    showToast(message);
    appendLog('[ERROR] ' + message);
  }
}

async function clearRuntimeMarker(){
  await runMaintenanceAction('clear_runtime_marker', 'runtime marker 已清理。');
}

async function resetNetworkState(){
  await runMaintenanceAction('reset_network_state', '系统代理残留已清理，运行态已回退。');
  if(invoke){
    settings = normalizeSettings(await invoke('get_settings'));
    renderSettingsPanel(settings);
    status = await invoke('app_status');
    renderStatus(status);
  }
}

async function validateCurrentConfig(){
  const result = await runMaintenanceAction('validate_current_config', '当前配置检查通过。');
  if(result?.path){
    showToast(`配置检查通过：${result.path}`);
  }
}

async function exportDiagnostics(){
  const result = await runMaintenanceAction('export_diagnostics', '诊断信息已导出。');
  if(result?.path){
    showToast(`诊断信息已导出到：${result.path}`);
  }
}

async function saveCurrentSettings(options = {}){
  settings = normalizeSettings(settings);
  if(!invoke || !settings){
    renderSettingsPanel(settings);
    renderSubscriptions(settings?.subscriptions || []);
    renderSelectedSubscriptionSummary();
    return;
  }
  try{
    settings = normalizeSettings(await invoke('save_settings', { settings }));
    renderSubscriptions(settings.subscriptions || []);
    renderSelectedSubscriptionSummary();
    renderSettingsPanel(settings);
    if(options.restartCore && status?.coreRunning){
      status = await invoke('restart_core');
      renderStatus(status);
      await refreshProxies();
      showToast('设置已保存，内核已重启生效。');
      return;
    }

    if(options.pendingOnly){
      showToast('设置已保存。该项当前仅保存偏好，后台能力待接入。');
    }
  }catch(err){
    showToast(formatError(err));
  }
}
