function filterNodes(query){
  nodeSearchQuery = String(query || '').trim().toLowerCase();
  renderProxyGroups();
}

function setNodeSourceFilter(filterValue){
  nodeSourceFilter = filterValue || 'all';
  renderProxyGroups();
}

function isProxyGroup(proxy){
  return Array.isArray(proxy?.all) && Boolean(proxy?.name);
}

function isInformationalNodeName(name){
  const normalized = String(name || '').trim().toLowerCase();
  if(!normalized) return true;

  const prefixes = [
    '剩余流量',
    '距离下次重置剩余',
    '套餐到期',
    '官网',
    '备用网址',
    '跳转域名',
    '请勿连接',
    '客服',
    '更新地址',
    '到期时间',
    '过期时间',
    '流量',
    '有效期',
    '联系',
    '电报群',
    'telegram',
    'tg群',
    '群组',
    '公告',
    '提示',
    '说明',
    '订阅',
    '获取更多',
    '购买',
    '续费',
    '网址',
    'http://',
    'https://'
  ];

  return prefixes.some(prefix => normalized.startsWith(prefix))
    || normalized.includes('请勿连接');
}

function getProxyDelayValue(proxy){
  const name = proxy?.name;
  if(name && nodeDelayOverrides.has(name)) return nodeDelayOverrides.get(name);

  const historyDelay = proxy?.history?.at?.(-1)?.delay;
  if(Number.isFinite(historyDelay)) return historyDelay;

  return Number.isFinite(proxy?.delay) ? proxy.delay : NaN;
}

function applySpeedTestResults(results){
  if(!Array.isArray(results)) return;

  for(const result of results){
    if(!result?.name) continue;
    if(Number.isFinite(result.delay)){
      nodeDelayOverrides.set(result.name, result.delay);
    } else {
      nodeDelayOverrides.delete(result.name);
    }
  }
}

async function loadSpeedTestCache(){
  if(!invoke) return;

  try{
    const results = await invoke('speed_test_cache');
    applySpeedTestResults(results);
  }catch(err){
    appendLog('[WARN] failed to load speed test cache: ' + formatError(err));
  }
}

function displayDelay(proxy){
  const delay = getProxyDelayValue(proxy);
  return Number.isFinite(delay) ? `${delay} ms` : '--';
}

function delayClass(delay){
  if(!Number.isFinite(delay)) return 'none';
  if(delay <= 120) return 'good';
  if(delay <= 250) return 'warn';
  return 'bad';
}

function getGroupTypeLabel(group){
  const raw = String(group?.type || 'Selector');
  if(raw === 'URLTest') return 'URL-Test';
  if(raw === 'Fallback') return 'Fallback';
  if(raw === 'Selector') return 'Selector';
  return raw;
}

function getEnabledSubscriptionList(){
  return enabledSubscriptions().map((subscription, index) => ({
    ...subscription,
    sortIndex: index,
    tags: Array.isArray(subscription.tags) ? subscription.tags : []
  }));
}

function buildNodeSourceMap(){
  const sourceMap = new Map();

  for(const subscription of getEnabledSubscriptionList()){
    for(const tag of subscription.tags){
      if(!sourceMap.has(tag)){
        sourceMap.set(tag, subscription);
      }
    }
  }

  return sourceMap;
}

function getNodeSource(node){
  return buildNodeSourceMap().get(node?.name) || null;
}

function getNodeSortScore(node, currentName){
  const delay = getProxyDelayValue(node);
  const activeScore = node.name === currentName ? 0 : 1;
  const delayScore = Number.isFinite(delay) ? delay : 999999;
  return [activeScore, delayScore, String(node.name || '').toLowerCase()];
}

function sortNodesForDisplay(nodes, currentName){
  return [...nodes].sort((left, right) => {
    const [leftActive, leftDelay, leftName] = getNodeSortScore(left, currentName);
    const [rightActive, rightDelay, rightName] = getNodeSortScore(right, currentName);

    if(leftActive !== rightActive) return leftActive - rightActive;
    if(leftDelay !== rightDelay) return leftDelay - rightDelay;
    return leftName.localeCompare(rightName, 'zh-CN');
  });
}

function buildProxyGroups(proxies){
  const sourceMap = buildNodeSourceMap();

  const groups = Object.values(proxies)
    .filter(isProxyGroup)
    .filter(group => !['GLOBAL'].includes(group.name));

  const mapped = groups.map(group => {
    const nodes = group.all
      .map(name => proxies[name])
      .filter(Boolean)
      .filter(node => !isProxyGroup(node))
      .filter(node => !['DIRECT', 'REJECT'].includes(node.name))
      .filter(node => !isInformationalNodeName(node.name))
      .map(node => ({
        ...node,
        source: sourceMap.get(node.name) || null
      }));

    const sortedNodes = sortNodesForDisplay(nodes, group.now);
    const sources = Array.from(
      new Map(
        sortedNodes
          .filter(node => node.source)
          .map(node => [node.source.id, node.source])
      ).values()
    );

    return {
      ...group,
      nodes: sortedNodes,
      sources
    };
  }).filter(group => group.nodes.length > 0);

  mapped.sort((a, b) => {
    if(a.name === 'PROXY') return -1;
    if(b.name === 'PROXY') return 1;
    if(a.name === currentGroup) return -1;
    if(b.name === currentGroup) return 1;
    return a.name.localeCompare(b.name, 'zh-CN');
  });

  return mapped;
}

function matchesNodeSource(node){
  if(nodeSourceFilter === 'all') return true;
  return node.source?.id === nodeSourceFilter;
}

function getFilteredNodes(group){
  const query = nodeSearchQuery;

  return group.nodes.filter(node => {
    if(!matchesNodeSource(node)) return false;
    if(!query) return true;

    const name = String(node.name || '').toLowerCase();
    const type = String(node.type || '').toLowerCase();
    const groupName = String(group.name || '').toLowerCase();
    const sourceName = String(node.source?.name || '').toLowerCase();

    return name.includes(query)
      || type.includes(query)
      || groupName.includes(query)
      || sourceName.includes(query);
  });
}

function getBestNode(group){
  const candidates = group.nodes
    .map(node => ({ node, delay: getProxyDelayValue(node) }))
    .filter(item => Number.isFinite(item.delay))
    .sort((a, b) => a.delay - b.delay);

  return candidates[0]?.node || null;
}

function ensureExpandedGroups(){
  if(!proxyGroupsState.length) return;

  if(expandedProxyGroups.size === 0){
    expandedProxyGroups.add(proxyGroupsState[0].name);
  }

  if(currentGroup){
    expandedProxyGroups.add(currentGroup);
  }
}

function toggleGroupExpansion(groupName){
  if(expandedProxyGroups.has(groupName)){
    expandedProxyGroups.delete(groupName);
  } else {
    expandedProxyGroups.add(groupName);
  }

  renderProxyGroups();
}

function renderNodeSourceFilters(){
  const container = ensureNodeSourceFiltersContainer();
  if(!container) return;

  const selected = getEnabledSubscriptionList();
  if(!selected.length){
    nodeSourceFilter = 'all';
    container.innerHTML = '';
    container.style.display = 'none';
    return;
  }

  container.style.display = 'flex';

  const options = [
    {
      value: 'all',
      label: '全部来源',
      count: selected.reduce((sum, subscription) => sum + (subscription.nodeCount || subscription.tags.length || 0), 0)
    },
    ...selected.map(subscription => ({
      value: subscription.id,
      label: subscription.name,
      count: subscription.nodeCount || subscription.tags.length || 0
    }))
  ];

  if(!options.some(option => option.value === nodeSourceFilter)){
    nodeSourceFilter = 'all';
  }

  container.innerHTML = options.map(option => `
    <button
      class="node-filter-chip ${nodeSourceFilter === option.value ? 'on' : ''}"
      onclick="setNodeSourceFilter('${attr(option.value)}')"
      title="${escapeHtml(option.label)}"
    >
      <span>${escapeHtml(option.label)}</span>
      <strong>${option.count}</strong>
    </button>
  `).join('');
}

function ensureNodeSourceFiltersContainer(){
  const existing = document.getElementById('node-source-filters');
  if(existing) return existing;

  const sourceBar = document.getElementById('node-source-bar');
  const host = sourceBar?.parentElement;
  if(!host) return null;

  host.classList.add('node-source-card');
  const container = document.createElement('div');
  container.id = 'node-source-filters';
  container.className = 'node-source-filters';
  host.appendChild(container);
  return container;
}

function ensureVisibleNodeStat(){
  const existing = document.getElementById('visible-node-count');
  if(existing) return existing;

  const stats = document.querySelector('.node-stats');
  if(!stats) return null;

  const stat = document.createElement('div');
  stat.className = 'node-stat';
  stat.innerHTML = '<div class="k">可见节点</div><div class="v" id="visible-node-count">0</div>';

  const activeGroupStat = document.getElementById('active-group-name')?.closest('.node-stat');
  if(activeGroupStat){
    stats.insertBefore(stat, activeGroupStat);
  } else {
    stats.appendChild(stat);
  }

  return stat.querySelector('#visible-node-count');
}

async function pickNode(groupName, name){
  if(!(invoke && status?.coreRunning)) return;

  try{
    await invoke('select_proxy', { request: { group: groupName, name } });
    appendLog(`[INFO] switched proxy group ${groupName} to ${name}`);
    currentGroup = groupName;
    expandedProxyGroups.add(groupName);
    await refreshProxies();
  }catch(err){
    showToast(formatError(err));
    appendLog('[ERROR] ' + formatError(err));
  }
}

async function testNode(name){
  if(!(invoke && status?.coreRunning)) return;

  nodeTestingState.add(name);
  renderProxyGroups();

  try{
    const delay = await invoke('delay_proxy', { name });
    nodeDelayOverrides.set(name, delay);
  }catch{
    nodeDelayOverrides.delete(name);
  }finally{
    nodeTestingState.delete(name);
    renderProxyGroups();
  }
}

async function testGroup(groupName){
  const group = proxyGroupsState.find(item => item.name === groupName);
  if(!(group && invoke && status?.coreRunning)) return;

  groupTestingState.add(groupName);
  renderProxyGroups();

  try{
    const results = await invoke('speed_test_nodes', {
      request: { names: group.nodes.map(node => node.name) }
    });
    applySpeedTestResults(results || []);
  }catch(err){
    showToast(formatError(err));
    appendLog('[ERROR] ' + formatError(err));
  }finally{
    groupTestingState.delete(groupName);
    renderProxyGroups();
  }
}

async function autoSelectBest(groupName){
  const group = proxyGroupsState.find(item => item.name === groupName);
  if(!(group && invoke && status?.coreRunning)) return;

  await testGroup(groupName);
  const best = getBestNode(group);

  if(!best){
    showToast('该分组暂时没有可用测速结果。');
    return;
  }

  await pickNode(groupName, best.name);
}

async function testAll(){
  const icon = document.getElementById('test-icon');

  if(!(invoke && status?.coreRunning)){
    showToast('内核未启动，无法测速。');
    return;
  }

  overallTesting = true;
  icon?.classList.add('ti-spin');
  proxyGroupsState.forEach(group => groupTestingState.add(group.name));
  renderProxyGroups();

  try{
    const names = proxyGroupsState.flatMap(group => group.nodes.map(node => node.name));
    const results = await invoke('speed_test_nodes', { request: { names } });
    applySpeedTestResults(results || []);
    showToast(`测速完成：${(results || []).filter(item => Number.isFinite(item.delay)).length} / ${(results || []).length}`);
  }catch(err){
    showToast(formatError(err));
    appendLog('[ERROR] ' + formatError(err));
  }finally{
    groupTestingState.clear();
    overallTesting = false;
    icon?.classList.remove('ti-spin');
    renderProxyGroups();
  }
}

async function refreshProxies(){
  renderSelectedSubscriptionSummary();

  if(enabledSubscriptions().length === 0){
    renderEmptyNodes('请先启用至少一个订阅。');
    return;
  }

  if(!invoke || !status?.coreRunning){
    renderEmptyNodes('内核未启动。');
    return;
  }

  try{
    await loadSpeedTestCache();
    const result = await invoke('list_proxies');
    proxySnapshot = result.raw?.proxies || {};
    proxyGroupsState = buildProxyGroups(proxySnapshot);
    currentGroup = proxyGroupsState.find(group => group.now)?.name || proxyGroupsState[0]?.name || 'PROXY';
    ensureExpandedGroups();
    renderProxyGroups();
  }catch(err){
    renderEmptyNodes('无法读取节点列表。');
    showToast(formatError(err));
  }
}

function renderProxyGroups(){
  const board = document.getElementById('node-list');
  if(!board) return;

  renderNodeSourceFilters();

  if(!proxyGroupsState.length){
    renderEmptyNodes('暂无可用分组。');
    return;
  }

  const visibleGroups = proxyGroupsState
    .map(group => ({ ...group, filteredNodes: getFilteredNodes(group) }))
    .filter(group => group.filteredNodes.length > 0);

  if(!visibleGroups.length){
    renderEmptyNodes('没有匹配的分组或节点。');
    return;
  }

  if(!visibleGroups.some(group => group.name === currentGroup)){
    currentGroup = visibleGroups[0].name;
    expandedProxyGroups.add(currentGroup);
  }

  const totalVisibleNodes = visibleGroups.reduce((sum, group) => sum + group.filteredNodes.length, 0);
  const currentGroupData = visibleGroups.find(group => group.name === currentGroup) || visibleGroups[0];
  const currentNode = currentGroupData.nodes.find(node => node.name === currentGroupData.now);
  const visibleNodeStat = ensureVisibleNodeStat();

  document.getElementById('available-node-count').textContent = String(visibleGroups.length);
  visibleNodeStat && (visibleNodeStat.textContent = String(totalVisibleNodes));
  document.getElementById('active-group-name').textContent = currentGroupData.name;

  const searchInput = document.getElementById('node-search-input');
  if(searchInput && searchInput.value !== nodeSearchQuery){
    searchInput.value = nodeSearchQuery;
  }

  document.getElementById('proxy-node').textContent = currentNode
    ? `${currentNode.name} · ${displayDelay(currentNode)}`
    : `${currentGroupData.now || currentGroupData.name} · 当前分组已选中`;

  document.getElementById('selected-sub-count')?.setAttribute(
    'title',
    `当前启用了 ${enabledSubscriptions().length} 个订阅`
  );
  document.getElementById('available-node-count')?.setAttribute(
    'title',
    `当前可见 ${visibleGroups.length} 个代理分组`
  );
  visibleNodeStat?.setAttribute('title', `当前可见 ${totalVisibleNodes} 个节点`);

  board.innerHTML = visibleGroups.map(group => groupCardHtml(group)).join('');
}

function renderEmptyNodes(text){
  renderNodeSourceFilters();
  const visibleNodeStat = ensureVisibleNodeStat();

  document.getElementById('available-node-count').textContent = '0';
  visibleNodeStat && (visibleNodeStat.textContent = '0');
  document.getElementById('active-group-name').textContent = currentGroup || 'PROXY';
  document.getElementById('proxy-node').textContent = text;
  document.getElementById('node-list').innerHTML = `
    <div class="proxy-group-card open">
      <div class="proxy-group-head">
        <div class="proxy-group-copy">
          <div class="proxy-group-title-row">
            <div class="proxy-group-title">${escapeHtml(text)}</div>
          </div>
          <div class="proxy-group-sub">启动内核并启用订阅后，这里会显示真实分组与节点。</div>
        </div>
      </div>
    </div>
  `;
}

function groupCardHtml(group){
  const isOpen = expandedProxyGroups.has(group.name);
  const currentNodeName = group.now || '未选择';
  const bestNode = getBestNode(group);
  const visibleNodes = isOpen ? group.filteredNodes : [];
  const typeLabel = getGroupTypeLabel(group);
  const testing = groupTestingState.has(group.name);
  const sourceNames = group.sources.slice(0, 2).map(source => source.name);
  const extraSourceCount = Math.max(group.sources.length - sourceNames.length, 0);

  return `
    <div class="proxy-group-card ${isOpen ? 'open' : ''}">
      <div class="proxy-group-head">
        <div class="proxy-group-copy">
          <div class="proxy-group-title-row">
            <div class="proxy-group-title">${escapeHtml(group.name)}</div>
            <span class="proxy-group-pill">${escapeHtml(typeLabel)}</span>
            <span class="proxy-group-pill count">${group.filteredNodes.length} / ${group.nodes.length} 节点</span>
          </div>
          <div class="proxy-group-sub">
            当前节点：<strong>${escapeHtml(currentNodeName)}</strong>
            ${bestNode ? ` · 推荐：<strong>${escapeHtml(bestNode.name)}</strong>` : ' · 先测速后可自动优选'}
          </div>
          <div class="proxy-group-meta-row">
            ${sourceNames.map(name => `<span class="proxy-group-meta-chip">${escapeHtml(name)}</span>`).join('')}
            ${extraSourceCount ? `<span class="proxy-group-meta-chip muted">+${extraSourceCount} 个来源</span>` : ''}
          </div>
        </div>
        <div class="proxy-group-actions">
          <button class="proxy-group-btn" onclick="autoSelectBest('${attr(group.name)}')">
            <i class="ti ti-circle-check"></i> 自动选择
          </button>
          <button class="proxy-group-btn ghost" onclick="testGroup('${attr(group.name)}')">
            <i class="ti ti-device-gamepad-2"></i> ${testing ? '测速中...' : '测速'}
          </button>
          <button class="proxy-group-toggle" onclick="toggleGroupExpansion('${attr(group.name)}')">
            <i class="ti ti-chevron-down"></i>
          </button>
        </div>
      </div>
      <div class="proxy-group-body">
        <div class="proxy-node-grid">
          ${visibleNodes.map(node => proxyNodeCardHtml(group, node)).join('')}
        </div>
      </div>
    </div>
  `;
}

function proxyNodeCardHtml(group, node){
  const delay = getProxyDelayValue(node);
  const stateClass = delayClass(delay);
  const active = group.now === node.name;
  const testing = nodeTestingState.has(node.name);
  const hasError = Number.isFinite(delay) && delay > 450;
  const sourceName = node.source?.name || '未标记来源';
  const meta = testing
    ? '测速中...'
    : Number.isFinite(delay)
      ? `延迟 ${delay} ms`
      : '点击测速';

  return `
    <div class="proxy-node-card ${active ? 'active' : ''} ${hasError ? 'error' : ''}" onclick="pickNode('${attr(group.name)}','${attr(node.name)}')">
      <div class="proxy-node-card-top">
        <div class="proxy-node-card-name">${escapeHtml(node.name)}</div>
        <span class="proxy-node-status ${stateClass}"></span>
      </div>
      <div class="proxy-node-card-tags">
        <span class="proxy-node-source">${escapeHtml(sourceName)}</span>
        <span class="proxy-node-kind">${escapeHtml(node.type || '节点')}</span>
      </div>
      <div class="proxy-node-card-meta">${active ? '当前使用中' : meta}</div>
      <div class="proxy-node-card-actions">
        <div class="proxy-node-card-delay">${displayDelay(node)}</div>
        <button class="proxy-node-test" onclick="event.stopPropagation(); testNode('${attr(node.name)}')">
          <i class="ti ti-device-gamepad-2"></i> ${testing ? '测速中' : '测速'}
        </button>
      </div>
    </div>
  `;
}
