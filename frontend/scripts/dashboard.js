const TRAFFIC_STORAGE_KEY = 'wepboxTrafficTotals';
const TRAFFIC_SAMPLE_SECONDS = 1.2;
const TRAFFIC_VIEW_MODES = ['session', 'month', 'total'];
const TRAFFIC_LABELS = {
  session: '本次流量',
  month: '本月流量',
  total: '总使用'
};

function currentTrafficMonthKey(){
  const now = new Date();
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}`;
}

function safeTrafficNumber(value){
  const numeric = Number(value);
  return Number.isFinite(numeric) && numeric > 0 ? numeric : 0;
}

function loadTrafficTotals(){
  const monthKey = currentTrafficMonthKey();
  let saved = {};

  try{
    saved = JSON.parse(localStorage.getItem(TRAFFIC_STORAGE_KEY) || '{}') || {};
  }catch(err){
    saved = {};
  }

  trafficTotals = {
    session: 0,
    month: saved.monthKey === monthKey ? safeTrafficNumber(saved.month) : 0,
    total: safeTrafficNumber(saved.total),
    monthKey
  };

  if(saved.monthKey !== monthKey){
    saveTrafficTotals();
  }
}

function saveTrafficTotals(){
  try{
    localStorage.setItem(TRAFFIC_STORAGE_KEY, JSON.stringify({
      month: Math.round(trafficTotals.month),
      total: Math.round(trafficTotals.total),
      monthKey: trafficTotals.monthKey || currentTrafficMonthKey()
    }));
  }catch(err){
    console.warn('save traffic totals failed', err);
  }
}

function ensureCurrentTrafficMonth(){
  const monthKey = currentTrafficMonthKey();
  if(trafficTotals.monthKey === monthKey) return;
  trafficTotals.month = 0;
  trafficTotals.monthKey = monthKey;
  saveTrafficTotals();
}

function formatTrafficBytes(bytes){
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = safeTrafficNumber(bytes);
  let unitIndex = 0;

  while(value >= 1024 && unitIndex < units.length - 1){
    value /= 1024;
    unitIndex += 1;
  }

  if(unitIndex === 0) return `${Math.round(value)} ${units[unitIndex]}`;
  const digits = value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(digits)} ${units[unitIndex]}`;
}

function renderTrafficSummary(){
  const label = document.getElementById('traffic-summary-label');
  const value = document.getElementById('traffic-summary-value');
  if(!label || !value) return;

  ensureCurrentTrafficMonth();
  label.textContent = TRAFFIC_LABELS[trafficViewMode] || TRAFFIC_LABELS.session;
  value.textContent = formatTrafficBytes(trafficTotals[trafficViewMode]);
}

function cycleTrafficSummary(){
  const index = TRAFFIC_VIEW_MODES.indexOf(trafficViewMode);
  trafficViewMode = TRAFFIC_VIEW_MODES[(index + 1) % TRAFFIC_VIEW_MODES.length];
  renderTrafficSummary();
}

function recordTrafficSample(up, down, seconds){
  ensureCurrentTrafficMonth();
  const bytes = (safeTrafficNumber(up) + safeTrafficNumber(down)) * 1024 * 1024 * seconds;
  if(bytes <= 0) {
    renderTrafficSummary();
    return;
  }

  trafficTotals.session += bytes;
  trafficTotals.month += bytes;
  trafficTotals.total += bytes;
  saveTrafficTotals();
  renderTrafficSummary();
}

function refreshChartSummary(index, maxValue){
  const point = chartPoints[index] || chartPoints[chartPoints.length - 1] || { up: 0, down: 0 };
  document.getElementById('chart-hover-time').textContent = index === chartPoints.length - 1 ? '当前' : `${chartPoints.length - 1 - index} 秒前`;
  document.getElementById('chart-hover-down').textContent = `下载 ${point.down.toFixed(1)} MB/s`;
  document.getElementById('chart-hover-up').textContent = `上传 ${point.up.toFixed(1)} MB/s`;
  document.querySelectorAll('.chart-bar').forEach((bar, barIndex) => {
    bar.classList.toggle('active', barIndex === index);
  });
}

function renderTrafficChart(){
  const maxPoint = chartPoints.reduce((currentMax, point) => Math.max(currentMax, point.up, point.down), 0);
  const maxValue = Math.max(Math.ceil(maxPoint), 3);
  const bars = document.getElementById('traffic-chart-bars');
  const activeIndex = chartHoverIndex >= 0 ? chartHoverIndex : chartPoints.length - 1;

  if(bars){
    bars.innerHTML = chartPoints.map((point, index) => {
      const value = Math.max(point.up, point.down);
      const height = Math.max(4, Math.round((value / maxValue) * 100));
      return `<div class="chart-bar ${index === activeIndex ? 'active' : ''}" style="height:${height}%"></div>`;
    }).join('');
  }

  document.getElementById('chart-scale-max').textContent = `${maxValue.toFixed(1)} MB/s`;
  document.getElementById('chart-scale-mid').textContent = `${(maxValue / 2).toFixed(1)} MB/s`;
  refreshChartSummary(activeIndex, maxValue);
}

function installChartHover(){
  const panel = document.getElementById('traffic-chart-panel');
  if(!panel) return;

  panel.addEventListener('mousemove', event => {
    const rect = panel.getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width));
    chartHoverIndex = Math.round(ratio * (chartPoints.length - 1));
    renderTrafficChart();
  });

  panel.addEventListener('mouseleave', () => {
    chartHoverIndex = -1;
    renderTrafficChart();
  });
}

function pushTrafficPoint(up, down){
  chartPoints.shift();
  chartPoints.push({ up, down });
}

function startDashboardTicker(){
  loadTrafficTotals();
  renderTrafficSummary();
  installChartHover();
  renderTrafficChart();

  if(invoke){
    setInterval(() => {
      loadStatus({ silent: true });
    }, 5000);
  }

  setInterval(() => {
    const isRunning = Boolean(status?.coreRunning || document.getElementById('main-tog').classList.contains('on'));
    const up = isRunning ? Number((Math.random() * 0.8 + 0.1).toFixed(1)) : 0;
    const down = isRunning ? Number((Math.random() * 2.5 + 0.2).toFixed(1)) : 0;

    document.getElementById('up-speed').innerHTML = `${up.toFixed(1)} <span class="su">MB/s</span>`;
    document.getElementById('dn-speed').innerHTML = `${down.toFixed(1)} <span class="su">MB/s</span>`;
    const cpu = isRunning ? Math.floor(Math.random() * 12 + 4) : Math.floor(Math.random() * 3 + 1);
    const mem = isRunning ? Math.floor(Math.random() * 4 + 18) : 14;
    document.getElementById('cpu-val').textContent = cpu + '%';
    document.getElementById('cpu-bar').style.width = cpu + '%';
    document.getElementById('mem-val').textContent = mem + '%';
    document.getElementById('mem-bar').style.width = mem + '%';

    recordTrafficSample(up, down, TRAFFIC_SAMPLE_SECONDS);
    pushTrafficPoint(up, down);
    renderTrafficChart();
  }, TRAFFIC_SAMPLE_SECONDS * 1000);
}
