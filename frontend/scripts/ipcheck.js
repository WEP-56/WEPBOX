const IP_CHECK_CUSTOM_TARGETS_KEY = 'wepboxIpCheckCustomTargets';
const DEFAULT_CONNECTIVITY_ITEMS = [
  { id: 'wechat', name: 'WeChat', status: 'pending' },
  { id: 'taobao', name: 'Taobao', status: 'pending' },
  { id: 'google', name: 'Google', status: 'pending' },
  { id: 'cloudflare', name: 'Cloudflare', status: 'pending' },
  { id: 'youtube', name: 'YouTube', status: 'pending' },
  { id: 'github', name: 'GitHub', status: 'pending' },
  { id: 'chatgpt', name: 'ChatGPT', status: 'pending' }
];
const CONNECTIVITY_ICON_MAP = {
  wechat: 'ti-message-circle',
  taobao: 'ti-building-store',
  google: 'ti-brand-chrome',
  cloudflare: 'ti-cloud',
  youtube: 'ti-brand-youtube',
  github: 'ti-brand-github',
  chatgpt: 'ti-brand-openai'
};

let ipCheckCustomTargets = loadIpCheckCustomTargets();
let ipCheckAddOpen = false;

function loadIpCheckCustomTargets(){
  try{
    const items = JSON.parse(localStorage.getItem(IP_CHECK_CUSTOM_TARGETS_KEY) || '[]');
    return Array.isArray(items) ? items.filter(validCustomTarget).slice(0, 8) : [];
  }catch(err){
    return [];
  }
}

function saveIpCheckCustomTargets(){
  localStorage.setItem(IP_CHECK_CUSTOM_TARGETS_KEY, JSON.stringify(ipCheckCustomTargets));
}

function validCustomTarget(target){
  return target
    && typeof target.name === 'string'
    && typeof target.url === 'string'
    && target.name.trim()
    && /^(https?:)\/\//.test(target.url.trim());
}

function ipValue(value, fallback = '-'){
  return value === null || value === undefined || value === '' ? fallback : value;
}

function ipCheckInitialResult(){
  return {
    checkedAt: null,
    ipv4: {
      version: 'IPv4',
      source: '等待检测',
      available: false,
      message: '点击刷新后开始检测。'
    },
    connectivity: [
      ...DEFAULT_CONNECTIVITY_ITEMS,
      ...ipCheckCustomTargets.map(target => ({ ...target, status: 'pending' }))
    ]
  };
}

function renderIpv4Card(card){
  const available = Boolean(card?.available);
  const regionLine = [card?.country, card?.region, card?.city].filter(Boolean).join(' / ');
  const quality = ipValue(card?.qualityScore, '未返回');
  const qualityNumber = Number(String(quality).split('/')[0]);
  const qualityWidth = Number.isFinite(qualityNumber) ? Math.max(4, Math.min(100, qualityNumber)) : 0;

  if(!available){
    return `
      <section class="ipcheck-card card muted">
        <div class="ipcheck-card-head">
          <div><span>1</span> IP 来源: ${escapeHtml(card?.source || 'IPv4')}</div>
          <button class="icon-btn" onclick="runIpCheck()" aria-label="刷新 IP 信息"><i class="ti ti-refresh"></i></button>
        </div>
        <div class="ipcheck-card-empty">
          <i class="ti ti-alert-circle"></i>
          <span>${escapeHtml(card?.message || '等待检测 IPv4 出口')}</span>
        </div>
      </section>
    `;
  }

  return `
    <section class="ipcheck-card card">
      <div class="ipcheck-card-head">
        <div><span>1</span> IP 来源: ${escapeHtml(card.source || 'IPv4')}</div>
        <button class="icon-btn" onclick="runIpCheck()" aria-label="刷新 IP 信息"><i class="ti ti-refresh"></i></button>
      </div>
      <div class="ipcheck-card-body">
        <div class="ipcheck-main-ip"><i class="ti ti-device-desktop"></i>${escapeHtml(ipValue(card.ip))}</div>
        <div class="ipcheck-loc-grid">
          <div><span><i class="ti ti-map-pin"></i>地区</span><strong>${escapeHtml(ipValue(card.country))}</strong></div>
          <div><span><i class="ti ti-home"></i>省份</span><strong>${escapeHtml(ipValue(card.region))}</strong></div>
          <div><span><i class="ti ti-arrow-guide"></i>城市</span><strong>${escapeHtml(ipValue(card.city))}</strong></div>
        </div>
        <div class="ipcheck-network">
          <span><i class="ti ti-router"></i>网络</span>
          <strong>${escapeHtml(ipValue(card.network || regionLine))}</strong>
        </div>
        <div class="ipcheck-quality-grid">
          <div><span><i class="ti ti-chart-line"></i>类型</span><strong>${escapeHtml(ipValue(card.usageType, '未知'))}</strong></div>
          <div><span><i class="ti ti-shield-check"></i>代理</span><strong>${escapeHtml(ipValue(card.proxy, '未返回'))}</strong></div>
          <div><span><i class="ti ti-home-check"></i>原生性</span><strong>${escapeHtml(ipValue(card.native, '未返回'))}</strong></div>
          <div class="ipcheck-score">
            <span><i class="ti ti-gauge"></i>IP 质量分</span>
            <div><b style="width:${qualityWidth}%"></b><strong>${escapeHtml(quality)}</strong></div>
          </div>
        </div>
        <div class="ipcheck-asn"><i class="ti ti-building-bank"></i><span>ASN</span><strong>${escapeHtml(ipValue(card.asn))}</strong></div>
      </div>
    </section>
  `;
}

function connectivityCardHtml(item){
  const icon = CONNECTIVITY_ICON_MAP[item.id] || 'ti-world';
  const statusClass = item.status || 'pending';
  const statusText = item.status === 'ok'
    ? '可用'
    : item.status === 'slow'
      ? '可用'
      : item.status === 'blocked'
        ? '不可用'
        : '待检测';
  const latency = item.latencyMs === null || item.latencyMs === undefined ? '' : `${item.latencyMs} ms`;

  return `
    <div class="connectivity-card ${escapeHtml(statusClass)}">
      <div class="connectivity-name"><i class="ti ${escapeHtml(icon)}"></i><strong>${escapeHtml(item.name)}</strong></div>
      <div class="connectivity-status">
        <span><i class="ti ${statusClass === 'blocked' ? 'ti-face-id-error' : 'ti-mood-smile'}"></i>${escapeHtml(statusText)}</span>
        <em>${escapeHtml(latency || item.message || '')}</em>
      </div>
    </div>
  `;
}

function renderIpCheck(){
  const resultEl = document.getElementById('ipcheck-result');
  const runBtn = document.getElementById('ipcheck-run-btn');
  if(!resultEl || !runBtn) return;

  const result = ipCheckResult || ipCheckInitialResult();
  runBtn.disabled = ipCheckLoading;
  runBtn.querySelector('span').textContent = ipCheckLoading ? '检测中...' : '刷新 IP 信息';

  resultEl.innerHTML = `
    <div class="ipcheck-compact-grid">
      ${renderIpv4Card(result.ipv4)}
      <section class="card ipcheck-connectivity-panel">
        <div class="connectivity-grid">
          ${(result.connectivity || []).map(connectivityCardHtml).join('')}
          ${ipCheckAddOpen ? ipCheckTargetFormHtml() : ipCheckAddButtonHtml()}
        </div>
      </section>
    </div>
  `;
}

function ipCheckAddButtonHtml(){
  return `
    <button class="connectivity-card add" onclick="openIpCheckTargetForm()" type="button">
      <i class="ti ti-circle-plus"></i>
      <span>添加测试</span>
    </button>
  `;
}

function ipCheckTargetFormHtml(){
  return `
    <div class="connectivity-card add-form">
      <input id="ipcheck-target-name" class="ipcheck-target-input" placeholder="名称" maxlength="32">
      <input id="ipcheck-target-url" class="ipcheck-target-input" placeholder="https://example.com/favicon.ico">
      <div class="ipcheck-target-actions">
        <button type="button" onclick="cancelIpCheckTargetForm()">取消</button>
        <button type="button" onclick="saveIpCheckTarget()">添加</button>
      </div>
    </div>
  `;
}

function openIpCheckTargetForm(){
  ipCheckAddOpen = true;
  renderIpCheck();
  setTimeout(() => document.getElementById('ipcheck-target-name')?.focus(), 0);
}

function cancelIpCheckTargetForm(){
  ipCheckAddOpen = false;
  renderIpCheck();
}

function saveIpCheckTarget(){
  const name = document.getElementById('ipcheck-target-name')?.value?.trim();
  const url = document.getElementById('ipcheck-target-url')?.value?.trim();
  if(!name || !url || !/^(https?:)\/\//.test(url)){
    showToast('请输入有效的 http(s) URL');
    return;
  }

  const target = {
    id: `custom-${Date.now()}`,
    name: name.trim().slice(0, 32),
    url: url.trim().slice(0, 512)
  };
  ipCheckCustomTargets.push(target);
  ipCheckCustomTargets = ipCheckCustomTargets.slice(-8);
  saveIpCheckCustomTargets();
  ipCheckAddOpen = false;
  renderIpCheck();
}

function ipCheckRequestPayload(){
  return {
    customTargets: ipCheckCustomTargets.map(target => ({
      id: target.id,
      name: target.name,
      url: target.url
    }))
  };
}

async function runIpCheck(){
  if(ipCheckLoading) return;
  ipCheckLoading = true;
  renderIpCheck();

  try{
    if(!invoke){
      ipCheckResult = {
        checkedAt: Math.floor(Date.now() / 1000),
        ipv4: {
          version: 'IPv4',
          source: 'Demo IPv4',
          available: true,
          ip: '203.0.113.8',
          country: 'Demo',
          region: 'WEPBOX',
          city: 'Local',
          network: 'Demo Network',
          usageType: '演示线路',
          proxy: '未识别为代理',
          native: '原生',
          qualityScore: '98/100',
          asn: 'AS64500'
        },
        connectivity: [
          { id: 'wechat', name: 'WeChat', status: 'ok', latencyMs: 33 },
          { id: 'taobao', name: 'Taobao', status: 'ok', latencyMs: 97 },
          { id: 'google', name: 'Google', status: 'slow', latencyMs: 135 },
          { id: 'cloudflare', name: 'Cloudflare', status: 'ok', latencyMs: 123 },
          { id: 'youtube', name: 'YouTube', status: 'slow', latencyMs: 412 },
          { id: 'github', name: 'GitHub', status: 'slow', latencyMs: 728 },
          { id: 'chatgpt', name: 'ChatGPT', status: 'blocked', message: 'timeout' },
          ...ipCheckCustomTargets.map(target => ({ ...target, status: 'pending' }))
        ]
      };
    } else {
      ipCheckResult = await invoke('run_ip_check', { request: ipCheckRequestPayload() });
    }
  }catch(err){
    showToast(formatError(err));
    appendLog('[ERROR] IP check failed: ' + formatError(err));
  }finally{
    ipCheckLoading = false;
    renderIpCheck();
  }
}

renderIpCheck();
