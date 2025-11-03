document.addEventListener('DOMContentLoaded', () => {
    const wifiList = document.getElementById('wifi-list');
    const scannerStatus = document.getElementById('scanner-status');
    const refreshBtn = document.getElementById('refresh-btn');
    const passwordModal = new bootstrap.Modal(document.getElementById('passwordModal'));
    const modalSsidName = document.getElementById('modal-ssid-name');
    const connectForm = document.getElementById('connect-form');
    const passwordInput = document.getElementById('password-input');
    const connectBtn = document.getElementById('connect-btn');
    const connectBtnText = document.getElementById('connect-btn-text');
    const connectSpinner = document.getElementById('connect-spinner');
    const connectionStatus = document.getElementById('connection-status');

    let selectedSsid = '';

    // --- Functions ---

    const showStatus = (isError, message) => {
        connectionStatus.innerHTML = `
            <div class="alert alert-${isError ? 'danger' : 'success'}" role="alert">
                ${message}
            </div>
        `;
        connectionStatus.classList.remove('d-none');
    };

    const setConnectingState = (isConnecting) => {
        passwordInput.disabled = isConnecting;
        connectBtn.disabled = isConnecting;
        if (isConnecting) {
            connectBtnText.textContent = '连接中...';
            connectSpinner.classList.remove('d-none');
            connectionStatus.classList.add('d-none');
        } else {
            connectBtnText.textContent = '连接';
            connectSpinner.classList.add('d-none');
        }
    };

    const fetchNetworks = async () => {
        wifiList.innerHTML = ''; // Clear previous list
        scannerStatus.classList.remove('d-none');
        wifiList.appendChild(scannerStatus);
        refreshBtn.disabled = true;
        refreshBtn.querySelector('i').classList.add('loading');

        try {
            const response = await fetch('/api/scan');
            if (!response.ok) {
                throw new Error(`HTTP error! status: ${response.status}`);
            }
            const networks = await response.json();

            scannerStatus.classList.add('d-none');
            if (networks.length === 0) {
                wifiList.innerHTML = '<div class="list-group-item">未找到任何网络。</div>';
            } else {
                networks.forEach(network => {
                    const item = document.createElement('button');
                    item.type = 'button';
                    item.className = 'list-group-item list-group-item-action d-flex justify-content-between align-items-center';
                    
                    // Determine Wi-Fi signal icon
                    let signalIcon;
                    if (network.signal > 75) {
                        signalIcon = 'bi-wifi';
                    } else if (network.signal > 50) {
                        signalIcon = 'bi-wifi-2';
                    } else if (network.signal > 25) {
                        signalIcon = 'bi-wifi-1';
                    } else {
                        signalIcon = 'bi-wifi-off'; // Or some other representation for low signal
                    }

                    // Determine lock icon
                    const lockIcon = network.security !== 'Open' ? '<i class="bi bi-lock-fill ms-2"></i>' : '';

                    item.innerHTML = `
                        <span>
                            <i class="bi ${signalIcon} me-2"></i>
                            ${network.ssid}
                        </span>
                        <span>
                            <small class="text-muted me-2">${network.security}</small>
                            ${lockIcon}
                        </span>
                    `;

                    item.dataset.ssid = network.ssid;
                    item.addEventListener('click', () => {
                        selectedSsid = network.ssid;
                        modalSsidName.textContent = network.ssid;
                        passwordInput.value = '';
                        connectionStatus.classList.add('d-none');
                        passwordModal.show();
                    });
                    wifiList.appendChild(item);
                });
            }
        } catch (error) {
            console.error('Failed to fetch networks:', error);
            scannerStatus.classList.add('d-none');
            wifiList.innerHTML = '<div class="list-group-item text-danger">扫描失败，请重试。</div>';
        } finally {
            refreshBtn.disabled = false;
            refreshBtn.querySelector('i').classList.remove('loading');
        }
    };

    const handleConnect = async (event) => {
        event.preventDefault();
        setConnectingState(true);

        const password = passwordInput.value;

        try {
            const response = await fetch('/api/connect', {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                },
                body: JSON.stringify({ ssid: selectedSsid, password }),
            });

            const result = await response.json();

            if (response.ok && result.status === 'success') {
                showStatus(false, '连接成功！设备将尝试连接到您的网络，此热点稍后将自动关闭。');
                // Hide buttons after success
                connectBtn.parentElement.innerHTML = '';
            } else {
                throw new Error(result.error || '连接失败，未知错误。');
            }
        } catch (error) {
            console.error('Connection failed:', error);
            showStatus(true, `连接失败：${error.message}`);
            setConnectingState(false);
        }
    };

    // --- Event Listeners ---
    refreshBtn.addEventListener('click', fetchNetworks);
    connectForm.addEventListener('submit', handleConnect);

    // Initial fetch
    fetchNetworks();
});
