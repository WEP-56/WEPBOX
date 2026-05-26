const CORE_RESTART_KEYS = new Set([
  'allowLan',
  'dnsGuardEnabled',
  'ipv6Enabled',
  'fakeDnsEnabled',
  'appRulesEnabled',
  'blockAdsEnabled',
  'tunAutoRoute',
  'tunStrictRoute'
]);

const PENDING_SETTING_KEYS = new Set([
]);

const systemThemeQuery = window.matchMedia?.('(prefers-color-scheme: light)');

function applyThemePreference(current = settings){
  const theme = normalizeThemeColor(current?.themeColor);
  document.documentElement.classList.toggle('light-theme', Boolean(current?.followSystemTheme));
  document.body.classList.remove('theme-mint', 'theme-blue', 'theme-cyan', 'theme-purple', 'theme-orange');
  document.body.classList.add(`theme-${theme}`);
}

if(systemThemeQuery?.addEventListener){
  systemThemeQuery.addEventListener('change', () => applyThemePreference(settings));
} else if(systemThemeQuery?.addListener){
  systemThemeQuery.addListener(() => applyThemePreference(settings));
}

function scrollToSettingsSection(sectionId){
  const buttonIndex = {
    general: 0,
    proxy: 1,
    rules: 2,
    advanced: 3,
    subscription: 4,
    maintenance: 5
  }[sectionId];

  document.querySelectorAll('.settings-section').forEach(section => {
    section.classList.toggle('active', section.id === `settings-section-${sectionId}`);
  });
  document.querySelectorAll('.settings-nav-btn').forEach((button, index) => {
    button.classList.toggle('active', index === buttonIndex);
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
  settings = normalizeSettings(settings);
  settings.mode = mode;
  renderMode(mode);
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

function normalizeThemeColor(value){
  return ['mint', 'blue', 'cyan', 'purple', 'orange'].includes(value) ? value : 'cyan';
}

async function updateThemeColor(themeColor){
  settings = normalizeSettings(settings);
  settings.themeColor = normalizeThemeColor(themeColor);
  renderSettingsPanel(settings);
  await saveCurrentSettings();
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

async function updateUserRouteRules(value){
  settings = normalizeSettings(settings);
  const raw = String(value || '').trim();
  if(!raw){
    settings.userRouteRules = [];
    renderSettingsPanel(settings);
    await saveCurrentSettings({ restartCore: true });
    return;
  }

  let parsed;
  try{
    parsed = JSON.parse(raw);
  }catch(err){
    renderSettingsPanel(settings);
    showToast('自定义规则必须是有效的 JSON 数组。');
    return;
  }

  if(!Array.isArray(parsed) || parsed.some(item => !item || typeof item !== 'object' || Array.isArray(item))){
    renderSettingsPanel(settings);
    showToast('自定义规则必须是对象数组。');
    return;
  }

  settings.userRouteRules = parsed;
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
  setToggleState('app-rules-tog', current.appRulesEnabled);
  setToggleState('block-ads-tog', current.blockAdsEnabled);
  setToggleState('experimental-quic-tog', current.experimentalQuic);
  setToggleState('tun-auto-route-tog', current.tunAutoRoute);
  setToggleState('tun-strict-route-tog', current.tunStrictRoute);
  document.getElementById('follow-system-theme-tog')?.removeAttribute('disabled');
  renderThemePicker(current.themeColor);

  setValue('fallback-select', current.fallback || 'proxy');
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
  setValue('user-route-rules-input', JSON.stringify(current.userRouteRules || [], null, 2));

  renderMaintenanceInfo();
}

function renderThemePicker(themeColor){
  const activeTheme = normalizeThemeColor(themeColor);
  document.querySelectorAll('[data-theme-color]').forEach(button => {
    button.classList.toggle('active', button.dataset.themeColor === activeTheme);
  });
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
  renderSingboxReleaseControls();
}

function renderSingboxReleaseControls(){
  const select = document.getElementById('singbox-release-select');
  const scanBtn = document.getElementById('singbox-scan-btn');
  const installBtn = document.getElementById('singbox-install-btn');
  const note = document.getElementById('singbox-release-note');

  if(select){
    if(singboxReleases.length){
      select.innerHTML = singboxReleases.map(item => {
        const size = item.assetSize ? ` · ${formatReleaseSize(item.assetSize)}` : '';
        const published = item.publishedAt ? ` · ${item.publishedAt.slice(0, 10)}` : '';
        return `<option value="${escapeHtml(item.version)}">v${escapeHtml(item.version)}${published}${size}</option>`;
      }).join('');
      select.value = selectedSingboxRelease || singboxReleases[0]?.version || '';
    } else {
      select.innerHTML = `<option value="">${singboxReleaseLoading ? '正在扫描...' : '先扫描版本'}</option>`;
      select.value = '';
    }
    select.disabled = singboxReleaseLoading || singboxInstallRunning || !singboxReleases.length;
  }

  if(scanBtn){
    scanBtn.textContent = singboxReleaseLoading ? '扫描中...' : '扫描版本';
    scanBtn.disabled = singboxReleaseLoading || singboxInstallRunning;
  }
  if(installBtn){
    installBtn.textContent = singboxInstallRunning ? '安装中...' : '安装所选版本';
    installBtn.disabled = singboxReleaseLoading || singboxInstallRunning || !selectedSingboxRelease;
  }
  if(note){
    if(singboxInstallRunning){
      note.textContent = '正在下载并替换 sing-box 内核，请不要关闭应用。';
    } else if(singboxReleases.length){
      note.textContent = `已扫描到 ${singboxReleases.length} 个可用版本，会自动停止内核并在替换完成后按原状态恢复运行。`;
    } else {
      note.textContent = '会自动停止内核并在替换完成后按原状态恢复运行。';
    }
  }
}

function formatReleaseSize(bytes){
  const value = Number(bytes) || 0;
  if(value <= 0) return '';
  if(value >= 1024 * 1024) return `${(value / 1024 / 1024).toFixed(1)} MB`;
  return `${Math.round(value / 1024)} KB`;
}

function selectSingboxRelease(version){
  selectedSingboxRelease = version || '';
  renderSingboxReleaseControls();
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

async function scanSingboxReleases(){
  if(!invoke){
    showToast('当前不在 Tauri 环境中，无法扫描内核版本。');
    return;
  }
  singboxReleaseLoading = true;
  renderSingboxReleaseControls();
  try{
    singboxReleases = await invoke('list_singbox_releases');
    selectedSingboxRelease = singboxReleases[0]?.version || '';
    renderSingboxReleaseControls();
    showToast(`已扫描到 ${singboxReleases.length} 个 sing-box 版本。`);
    appendLog(`[INFO] sing-box releases loaded: ${singboxReleases.map(item => item.version).join(', ')}`);
  }catch(err){
    const message = formatError(err);
    showToast(message);
    appendLog('[ERROR] ' + message);
  }finally{
    singboxReleaseLoading = false;
    renderSingboxReleaseControls();
  }
}

async function installSelectedSingboxRelease(){
  if(!invoke){
    showToast('当前不在 Tauri 环境中，无法安装内核。');
    return;
  }
  if(!selectedSingboxRelease){
    showToast('请先扫描并选择一个 sing-box 版本。');
    return;
  }
  if(!window.confirm(`将下载并替换当前 sing-box 内核为 v${selectedSingboxRelease}。继续？`)) return;

  singboxInstallRunning = true;
  renderSingboxReleaseControls();
  try{
    const result = await invoke('install_singbox_release', { version: selectedSingboxRelease });
    showToast(result?.message || 'sing-box 内核已更新。');
    appendLog(`[INFO] ${result?.message || 'sing-box core updated'}${result?.path ? `: ${result.path}` : ''}`);
    await refreshMaintenanceInfo();
    if(invoke){
      status = await invoke('app_status');
      renderStatus(status);
      if(status?.coreRunning) await refreshProxies();
    }
  }catch(err){
    const message = formatError(err);
    showToast(message);
    appendLog('[ERROR] ' + message);
  }finally{
    singboxInstallRunning = false;
    renderSingboxReleaseControls();
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

queueMicrotask(() => scrollToSettingsSection('general'));
