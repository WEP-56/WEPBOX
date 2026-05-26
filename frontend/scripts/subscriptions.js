async function toggleSubscriptionSelection(id){
  settings = normalizeSettings(settings);
  const subscription = settings.subscriptions.find(item => item.id === id);
  if(!subscription) return;
  subscription.enabled = !subscription.enabled;
  renderSubscriptions(settings.subscriptions);
  renderSelectedSubscriptionSummary();
  await saveCurrentSettings({ restartCore: true });
  if(!status?.coreRunning) renderEmptyNodes(subscription.enabled ? '内核未启动' : '已取消订阅勾选');
}

async function refreshSubscriptionItem(id){
  if(!invoke) return;
  const subscription = settings?.subscriptions?.find(item => item.id === id);
  if(!subscription) return;
  if(!isRemoteSubscription(subscription)){
    showToast('手动导入的节点不支持自动更新。');
    return;
  }
  try{
    appendLog(`[INFO] refreshing subscription: ${subscription.name}`);
    const result = await invoke('refresh_subscription', { id });
    settings = normalizeSettings(await invoke('get_settings'));
    status = await invoke('app_status');
    renderStatus(status);
    renderSubscriptions(settings.subscriptions);
    renderSelectedSubscriptionSummary();
    showToast(`订阅已更新：${result.nodeCount} 个节点${result.restarted ? '，内核已重启' : ''}`);
    if(status.coreRunning) await refreshProxies();
  }catch(err){
    showToast(formatError(err));
    appendLog('[ERROR] ' + formatError(err));
  }
}

async function deleteSubscriptionItem(id){
  const subscription = settings?.subscriptions?.find(item => item.id === id);
  if(!subscription) return;
  if(!window.confirm(`删除订阅“${subscription.name}”？`)) return;
  try{
    await invoke?.('delete_subscription', { id });
    settings = normalizeSettings(await invoke('get_settings'));
    status = await invoke('app_status');
    renderStatus(status);
    renderSubscriptions(settings.subscriptions);
    renderSelectedSubscriptionSummary();
    appendLog(`[INFO] subscription deleted: ${subscription.name}`);
    showToast('订阅已删除');
    if(status.coreRunning) {
      await refreshProxies();
    } else if(!settings.subscriptions.length) {
      renderEmptyNodes('暂无可用节点');
    }
  }catch(err){
    showToast(formatError(err));
    appendLog('[ERROR] ' + formatError(err));
  }
}

function showModal(){ document.getElementById('modal-layer').classList.add('on'); }
function hideModal(){ document.getElementById('modal-layer').classList.remove('on'); }

async function fakeImport(){
  const input = document.getElementById('sub-input');
  const nameInput = document.getElementById('sub-name-input');
  const url = input.value.trim();
  const name = nameInput?.value.trim() || '';
  if(!url){
    input.focus();
    showToast('请先粘贴订阅链接。');
    return;
  }
  if(!invoke){
    hideModal();
    showToast('当前不在 Tauri 环境中，无法导入订阅。');
    return;
  }
  try{
    appendLog('[INFO] downloading subscription and preparing sing-box outbounds');
    const request = name ? { url, name } : { url };
    const result = await invoke('import_subscription', { request });
    settings = normalizeSettings(await invoke('get_settings'));
    status = await invoke('app_status');
    renderStatus(status);
    renderSubscriptions(settings.subscriptions || []);
    renderSelectedSubscriptionSummary();
    renderSettingsPanel(settings);
    hideModal();
    input.value = '';
    if(nameInput) nameInput.value = '';
    showToast(`订阅已导入：${result.nodeCount} 个节点${result.restarted ? '，内核已重启' : ''}`);
    appendLog(`[INFO] subscription imported: ${result.subscription.name}, nodes=${result.nodeCount}`);
    if(status.coreRunning) await refreshProxies();
  }catch(err){
    showToast(formatError(err));
    appendLog('[ERROR] ' + formatError(err));
  }
}

async function renameSubscriptionItem(id){
  if(!invoke) return;
  const subscription = settings?.subscriptions?.find(item => item.id === id);
  if(!subscription) return;
  const nextName = window.prompt('订阅名称', subscription.name)?.trim();
  if(!nextName || nextName === subscription.name) return;

  try{
    const updated = await invoke('rename_subscription', { request: { id, name: nextName } });
    settings = normalizeSettings(await invoke('get_settings'));
    renderSubscriptions(settings.subscriptions);
    renderSelectedSubscriptionSummary();
    renderSettingsPanel(settings);
    showToast(`订阅已重命名：${updated.name}`);
  }catch(err){
    showToast(formatError(err));
    appendLog('[ERROR] ' + formatError(err));
  }
}

function renderSubscriptions(subscriptions){
  const list = document.getElementById('subscription-list');
  if(!list) return;
  if(!subscriptions.length){
    list.innerHTML = `<div class="sub-row"><div class="sub-copy"><div class="rn">尚未导入订阅</div><div class="rs">粘贴 Clash / V2Ray / sing-box 订阅链接后会在这里显示</div></div><span class="badge ba">等待</span></div>`;
    return;
  }
  list.innerHTML = subscriptions.map(subscription => {
    const ok = subscription.status === 'active';
    const enabled = subscription.enabled !== false;
    return `<div class="sub-row ${enabled ? '' : 'off'}">
      <button class="sub-check ${enabled ? 'on' : ''}" onclick="toggleSubscriptionSelection('${attr(subscription.id)}')" aria-label="切换订阅选择">
        <i class="ti ti-check"></i>
      </button>
      <div class="sub-copy">
        <div class="rn">${escapeHtml(subscription.name)}</div>
        <div class="sub-meta">
          <span class="sub-note">${subscription.nodeCount || 0} 个节点</span>
          <span class="sub-note">${formatUpdatedAt(subscription.updatedAt)}</span>
          <span class="sub-note">${isRemoteSubscription(subscription) ? '远程订阅' : '手动导入'}</span>
        </div>
      </div>
      <div class="sub-actions">
        <span class="badge ${ok ? 'bg' : 'ba'}">${ok ? '正常' : '失败'}</span>
        ${isRemoteSubscription(subscription) ? `<button class="icon-btn" onclick="refreshSubscriptionItem('${attr(subscription.id)}')" aria-label="更新订阅"><i class="ti ti-refresh"></i></button>` : ''}
        <button class="icon-btn" onclick="renameSubscriptionItem('${attr(subscription.id)}')" aria-label="重命名订阅"><i class="ti ti-pencil"></i></button>
        <button class="icon-btn" onclick="deleteSubscriptionItem('${attr(subscription.id)}')" aria-label="删除订阅"><i class="ti ti-trash"></i></button>
      </div>
    </div>`;
  }).join('');
}

function renderSelectedSubscriptionSummary(){
  const selected = enabledSubscriptions();
  const bar = document.getElementById('node-source-bar');
  const summary = document.getElementById('subscription-source-summary');
  if(bar){
    if(!selected.length){
      bar.innerHTML = `<span class="sub-chip"><strong>未选择订阅</strong> 去订阅页勾选要参与节点列表的订阅</span>`;
    } else {
      bar.innerHTML = selected.map(subscription => `<span class="sub-chip"><strong>${escapeHtml(subscription.name)}</strong><span>${subscription.nodeCount || 0} 节点</span></span>`).join('');
    }
  }
  if(summary){
    if(!selected.length){
      summary.innerHTML = `<span class="sub-chip"><strong>未选择订阅</strong> 去勾选要参与代理的订阅来源</span>`;
    } else {
      summary.innerHTML = selected.map(subscription => `<span class="sub-chip"><strong>${escapeHtml(subscription.name)}</strong><span>${subscription.nodeCount || 0} 节点</span></span>`).join('');
    }
  }
  const selectedCount = document.getElementById('selected-sub-count');
  if(selectedCount){
    selectedCount.textContent = String(selected.length);
  }
  if(typeof renderNodeSourceFilters === 'function'){
    renderNodeSourceFilters();
  }
}

function formatUpdatedAt(value){
  if(!value) return '未同步';
  const date = new Date(value * 1000);
  return Number.isNaN(date.getTime()) ? '刚刚同步' : date.toLocaleString();
}
