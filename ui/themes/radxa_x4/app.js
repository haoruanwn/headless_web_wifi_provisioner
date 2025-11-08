/* Radxa X4 UI script - self-contained, simple API calls to /api/scan and /api/connect */
document.addEventListener('DOMContentLoaded', () => {
  const wifiList = document.getElementById('wifi-list');
  const refreshBtn = document.getElementById('refresh-btn');
  const modal = document.getElementById('passwordModal');
  const modalSsid = document.getElementById('modal-ssid-name');
  const passwordInput = document.getElementById('password-input');
  const connectForm = document.getElementById('connect-form');
  const connectBtn = document.getElementById('connect-btn');
  const cancelBtn = document.getElementById('cancel-btn');
  const connectionStatus = document.getElementById('connection-status');

  let selectedSsid = null;
  let backendKind = 'unknown';

  function showScannerStatus(text){
    wifiList.innerHTML = `<div class="scanner-status"><div class="spinner"></div><div class="scanner-text">${text}</div></div>`;
  }

  async function fetchWifiNetworks(){
    showScannerStatus('正在扫描 Wi‑Fi...');
    refreshBtn.disabled = true;
    try {
      const res = await fetch('/api/scan');
      if(!res.ok) throw new Error('扫描失败: ' + res.status);
      const nets = await res.json();
      renderList(nets);
    } catch(err){
      // 如果是网络错误（例如 TDM 模式短暂不可达），显示提示并重试
      console.warn('scan error', err);
      showScannerStatus('扫描失败，7秒后重试...');
      setTimeout(fetchWifiNetworks, 7000);
    } finally{
      refreshBtn.disabled = false;
    }
  }

  function renderList(nets){
    if(!nets || nets.length === 0){
      showScannerStatus('未找到可用网络');
      return;
    }
    wifiList.innerHTML = '';
    nets.forEach(n => {
      const el = document.createElement('div');
      el.className = 'network-item';
      const bars = signalBarsHtml(n.signal);
      el.innerHTML = `
        <div class="network-left">
          <img class="wifi-svg" src="assets/wifi.svg" alt="wifi">
          <div>
            <div class="net-ssid">${escapeHtml(n.ssid)}</div>
            <div class="net-meta">${n.security} • 信号 ${n.signal}%</div>
          </div>
        </div>
        <div class="net-right">
          <div class="net-signal">${n.signal}%</div>
          ${bars}
        </div>
      `;
      el.addEventListener('click', () => {
        if(n.security && n.security !== 'Open'){
          openModal(n.ssid);
        } else {
          connect(n.ssid, '');
        }
      });
      wifiList.appendChild(el);
    });
  }

  function openModal(ssid){
    selectedSsid = ssid;
    modalSsid.textContent = `连接 ${ssid}`;
    passwordInput.value = '';
    connectionStatus.textContent = '';
    modal.style.display = 'flex';
    modal.setAttribute('aria-hidden', 'false');
    // trap focus to first input
    setTimeout(()=>passwordInput.focus(),50);
  }
  function closeModal(){
    modal.style.display = 'none';
    modal.setAttribute('aria-hidden', 'true');
  }

  async function connect(ssid, password){
    connectBtn.disabled = true;
    connectionStatus.textContent = '正在连接...';
    try{
      const res = await fetch('/api/connect', {
        method: 'POST',
        headers:{'Content-Type':'application/json'},
        body: JSON.stringify({ssid, password})
      });
      if(!res.ok){
        const e = await res.json().catch(()=>({error:'连接失败'}));
        throw new Error(e.error || '连接失败');
      }
      connectionStatus.textContent = '连接成功，设备将退出配网模式';
      connectionStatus.style.color = '#6ee7b7';
      setTimeout(closeModal, 2000);
    }catch(err){
      connectionStatus.textContent = '连接失败：' + err.message;
      connectionStatus.style.color = '#ff6b6b';
      connectBtn.disabled = false;
    }
  }

  connectForm.addEventListener('submit', (ev)=>{
    ev.preventDefault();
    const pwd = passwordInput.value || '';
    connect(selectedSsid, pwd);
  });
  cancelBtn.addEventListener('click', closeModal);
  refreshBtn.addEventListener('click', fetchWifiNetworks);
  // modal close button
  const modalClose = document.getElementById('modal-close');
  if(modalClose){ modalClose.addEventListener('click', closeModal); }

  // helper to render signal bars
  function signalBarsHtml(signal){
    const level = Math.max(0, Math.min(4, Math.ceil((signal/100)*4)));
    let html = '<div class="signal-bars" aria-hidden="true">';
    for(let i=1;i<=4;i++){
      html += `<span class="${i<=level? 'active':''}" style="height:${6 + i*6}px"></span>`;
    }
    html += '</div>';
    return html;
  }

  // small helper
  function escapeHtml(s){
    return String(s).replace(/[&<>"']/g, c=>({
      '&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":"&#39;"}[c]));
  }

  // detect backend kind and hide refresh for TDM
  (async () => {
    try{
      const res = await fetch('/api/backend_kind');
      if(res.ok){
        const j = await res.json();
        backendKind = j.kind || 'unknown';
        if(backendKind === 'tdm' && refreshBtn){
          refreshBtn.style.display = 'none';
        }
      }
    }catch(e){
      // ignore
    } finally {
      fetchWifiNetworks();
    }
  })();
});
