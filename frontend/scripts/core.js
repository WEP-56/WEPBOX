async function go(name){
  pages.forEach(pageName => document.getElementById('pg-' + pageName).classList.remove('on'));
  document.getElementById('pg-' + name).classList.add('on');

  const idx = { home: 0, nodes: 1, subscriptions: 2, settings: 3, ipcheck: 4 }[name];
  navBtns.forEach(button => button.classList.remove('on'));
  if(idx !== undefined) navBtns[idx].classList.add('on');

  if(name === 'nodes') await refreshProxies();
}

async function toggleProxy(){
  if(!invoke){
    renderStatus({
      coreRunning: !document.getElementById('main-tog').classList.contains('on'),
      coreHealthy: true,
      localMixedPort: 7890,
      apiBaseUrl: 'demo'
    });
    return;
  }

  const shouldStop = document.getElementById('main-tog').classList.contains('on');
  setBusy(true);

  try{
    appendLog(shouldStop ? '[INFO] stopping sing-box core by user request' : '[INFO] starting sing-box core sidecar');
    status = await invoke(shouldStop ? 'stop_core' : 'start_core');
    renderStatus(status);
    appendLog(status.coreRunning ? '[INFO] core started and clash api is ready' : '[INFO] core stopped');
    if(status.coreRunning) await refreshProxies();
  }catch(err){
    showToast(formatError(err));
    appendLog('[ERROR] ' + formatError(err));
    await loadStatus();
  }finally{
    setBusy(false);
  }
}

async function loadStatus(options = {}){
  if(!invoke) return;

  try{
    status = await invoke('app_status');
    renderStatus(status);
  }catch(err){
    if(!options.silent) showToast(formatError(err));
  }
}

function renderStatus(next){
  status = next;
  const on = Boolean(next.coreRunning);
  const healthy = !on || next.coreHealthy !== false;
  const lastExit = next.coreLastExit || '';

  if(lastExit && lastExit !== lastCoreExitMessage){
    lastCoreExitMessage = lastExit;
    appendLog(`[WARN] ${lastExit}; runtime was moved back to a safe state`);
    if(settings?.notifyOnFailure){
      showToast('sing-box 上次异常退出，应用已回退到安全状态。');
    }
  }

  document.getElementById('core-control-card')?.classList.toggle('active', on && healthy);
  document.getElementById('main-tog').classList.toggle('on', on);
  document.getElementById('proxy-label').textContent = on
    ? (healthy ? '代理已启动' : '代理状态异常')
    : '代理服务已关闭';
  document.getElementById('proxy-node').textContent = on
    ? (healthy ? '内核运行中，Clash API 可访问' : '内核进程存在，但 Clash API 未响应')
    : (lastExit ? '上次内核异常退出，已回退到安全状态' : `本地监听端口 ${next.localMixedPort || 7890}`);
  document.getElementById('home-port').textContent = String(next.localMixedPort || 7890);
  if(typeof next.tunEnabled === 'boolean') renderTun(next.tunEnabled);

  if(!on) renderEmptyNodes('内核未启动。');
}

function renderTun(on){
  document.getElementById('tun-mode-btn').classList.toggle('on', on);
  document.getElementById('tun-card')?.classList.toggle('on', on);
  document.getElementById('tun-sub').textContent = on ? '已开启 · 接管系统流量，按当前模式分流' : '关闭 · 仅代理端口生效';
}

function renderMode(mode){
  const labelMap = {
    rule: '规则',
    global: '全局',
    direct: '直连'
  };

  document.querySelectorAll('[data-mode]').forEach(button => {
    button.classList.toggle('on', button.dataset.mode === mode);
  });
  const labelEl = document.getElementById('home-mode-label');
  if(labelEl) labelEl.textContent = labelMap[mode] || mode || '规则';
}
