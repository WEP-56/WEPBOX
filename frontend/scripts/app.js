document.querySelector('.titlebar')?.addEventListener('mousedown', async (event) => {
  if(event.button !== 0) return;
  if(event.target.closest('.window-actions')) return;
  try{
    await tauriWindow?.startDragging?.();
  }catch(err){
    console.warn('startDragging failed', err);
  }
});

async function boot(){
  if(!invoke){
    appendLog('[WARN] running outside Tauri; UI is in demo mode');
    settings = normalizeSettings();
    renderStatus({ coreRunning: false, coreHealthy: false, localMixedPort: 7890, apiBaseUrl: 'demo' });
    renderMode(settings.mode);
    renderTun(settings.tunEnabled);
    renderSubscriptions(settings.subscriptions || []);
    renderSelectedSubscriptionSummary();
    renderSettingsPanel(settings);
    if(typeof refreshMaintenanceInfo === 'function') await refreshMaintenanceInfo();
    return;
  }
  try{
    settings = normalizeSettings(await invoke('get_settings'));
    status = await invoke('app_status');
    renderStatus(status);
    renderMode(settings.mode);
    renderTun(settings.tunEnabled);
    renderSubscriptions(settings.subscriptions || []);
    renderSelectedSubscriptionSummary();
    renderSettingsPanel(settings);
    if(typeof refreshMaintenanceInfo === 'function') await refreshMaintenanceInfo();
    appendLog('[INFO] Tauri backend connected');
    if(status.coreRunning) {
      await refreshProxies();
    } else if(settings.resumeAfterElevation) {
      appendLog('[INFO] restoring proxy runtime after elevation');
      try{
        status = await invoke('start_core');
        renderStatus(status);
        if(status.coreRunning) await refreshProxies();
      } finally {
        settings.resumeAfterElevation = false;
        settings = normalizeSettings(await invoke('save_settings', { settings }));
      }
    } else if(settings.autoStartProxy && settings.proxyEnabled) {
      appendLog('[INFO] restoring proxy runtime after relaunch');
      status = await invoke('start_core');
      renderStatus(status);
      if(status.coreRunning) await refreshProxies();
    }
  }catch(err){
    showToast(formatError(err));
    appendLog('[ERROR] ' + formatError(err));
  }
}

startDashboardTicker();
boot();
