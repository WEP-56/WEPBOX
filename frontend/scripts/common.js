const logEntries = [];

function setBusy(busy){
  document.getElementById('main-tog').disabled = busy;
}

function showToast(message){
  const el = document.getElementById('toast');
  el.textContent = message;
  el.classList.add('on');
  clearTimeout(showToast.timer);
  showToast.timer = setTimeout(() => el.classList.remove('on'), 4800);
}

function appendLog(message){
  logEntries.push(message);
  while(logEntries.length > 24) logEntries.shift();
  renderLogBox();
}

function toggleLogExpanded(){
  logExpanded = !logExpanded;
  renderLogBox();
}

function renderLogBox(){
  const box = document.getElementById('log-box');
  const toggle = document.getElementById('log-toggle');
  const label = document.getElementById('log-toggle-label');
  if(!box) return;

  box.classList.toggle('collapsed', !logExpanded);
  box.innerHTML = logEntries.map(message => `<div class="log-line">${escapeHtml(message)}</div>`).join('');

  if(toggle){
    toggle.setAttribute('aria-expanded', String(logExpanded));
  }
  if(label){
    label.textContent = logExpanded ? '收起' : '展开';
  }
}

function initializeLogBox(){
  const lines = Array.from(document.querySelectorAll('#log-box .log-line')).map(item => item.textContent || '');
  logEntries.splice(0, logEntries.length, ...lines);
  renderLogBox();
}

function formatError(err){
  const text = String(err?.message || err);
  if(text.includes('os error 2')) return '未找到 sing-box 内核文件，请确认 sidecar 已放入 binaries 目录。';
  if(text.includes('Connection refused')) return '内核 API 暂不可用，请确认 sing-box 已启动。';
  if(text.includes('did not become ready')) return '内核已启动但 API 未就绪，可能是端口被占用或配置异常。';
  if(text.includes('当前仅支持 sing-box 原生 JSON')) return text;
  return text;
}

function escapeHtml(value){
  return String(value).replace(/[&<>"']/g, char => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[char]));
}

function attr(value){
  return String(value).replace(/\\/g, '\\\\').replace(/'/g, "\\'");
}

initializeLogBox();
