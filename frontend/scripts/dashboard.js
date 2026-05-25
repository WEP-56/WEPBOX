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

    pushTrafficPoint(up, down);
    renderTrafficChart();
  }, 1200);
}
