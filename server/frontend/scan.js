import init, { verify_ticket_wasm } from '/wasm/spinwin_scanner.js';

let publicKey = null;
let wasmReady = false;
let scanning = true;
let lastScannedToken = null;

async function setup() {
    // Load WASM module
    try {
        await init({ module_or_path: '/wasm/spinwin_scanner_bg.wasm' });
        wasmReady = true;
    } catch (e) {
        console.warn('WASM load failed, falling back to server verification', e);
    }

    // Fetch public key
    try {
        const res = await fetch('/api/public-key');
        publicKey = await res.text();
    } catch (e) {
        console.error('Failed to fetch public key', e);
    }

    // Start camera
    try {
        if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
            throw new Error('Camera API not available. If not on HTTPS, try localhost or enable secure context.');
        }
        const stream = await navigator.mediaDevices.getUserMedia({
            video: { facingMode: { ideal: 'environment' } }
        });
        const video = document.getElementById('scanner-video');
        video.srcObject = stream;
        await video.play();
        requestAnimationFrame(scanFrame);
    } catch (e) {
        console.error('Camera error:', e);
        const msg = e.name === 'NotAllowedError'
            ? 'Camera access denied. Please allow camera access in your browser settings.'
            : e.name === 'NotFoundError'
            ? 'No camera found on this device.'
            : e.name === 'NotReadableError'
            ? 'Camera is in use by another app. Close other apps using the camera and refresh.'
            : 'Camera error: ' + e.message;
        document.getElementById('status-line').textContent = msg;
    }
}

function scanFrame() {
    if (!scanning) {
        requestAnimationFrame(scanFrame);
        return;
    }

    const video = document.getElementById('scanner-video');
    if (video.readyState !== video.HAVE_ENOUGH_DATA) {
        requestAnimationFrame(scanFrame);
        return;
    }

    const canvas = document.createElement('canvas');
    canvas.width = video.videoWidth;
    canvas.height = video.videoHeight;
    const ctx = canvas.getContext('2d');
    ctx.drawImage(video, 0, 0);
    const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);

    const code = jsQR(imageData.data, canvas.width, canvas.height);
    if (code && code.data && code.data !== lastScannedToken) {
        lastScannedToken = code.data;
        handleScan(code.data);
    }

    requestAnimationFrame(scanFrame);
}

async function handleScan(token) {
    scanning = false;
    const overlay = document.getElementById('scan-overlay');

    // Step 1: Client-side signature verification via WASM (instant)
    let localResult = null;
    if (wasmReady && publicKey) {
        localResult = verify_ticket_wasm(publicKey, token);
    }

    if (localResult && !localResult.valid) {
        showResult('invalid', localResult, null);
        overlay.className = 'scan-overlay invalid';
        return;
    }

    // Step 2: Server verification (checks redemption status)
    try {
        const res = await fetch(`/api/verify/${encodeURIComponent(token)}`);
        const serverResult = await res.json();

        if (!serverResult.valid) {
            showResult('invalid', null, serverResult);
            overlay.className = 'scan-overlay invalid';
        } else if (serverResult.redeemed) {
            showResult('redeemed', localResult, serverResult);
            overlay.className = 'scan-overlay redeemed';
        } else {
            showResult('valid', localResult, serverResult);
            overlay.className = 'scan-overlay valid';
        }
    } catch (e) {
        // Offline — rely on WASM result
        if (localResult && localResult.valid) {
            showResult('valid-offline', localResult, null);
            overlay.className = 'scan-overlay valid';
        } else {
            showResult('error', null, null);
            overlay.className = 'scan-overlay invalid';
        }
    }
}

function showResult(status, local, server) {
    const panel = document.getElementById('result-panel');
    const statusEl = document.getElementById('result-status');
    const detailsEl = document.getElementById('result-details');
    const redeemBtn = document.getElementById('redeem-btn');

    panel.classList.add('show');
    redeemBtn.style.display = 'none';

    const info = server || local || {};

    if (status === 'valid' || status === 'valid-offline') {
        statusEl.textContent = status === 'valid-offline' ? 'VALID (Offline)' : 'VALID';
        statusEl.className = 'result-status status-valid';
        detailsEl.innerHTML = `
            <div class="result-detail"><span>Prize:</span> ${info.prize || 'Unknown'}</div>
            <div class="result-detail"><span>Attendee:</span> ${info.attendee || info.name || 'Unknown'}</div>
        `;
        redeemBtn.style.display = 'block';
        redeemBtn.disabled = false;
        redeemBtn.textContent = 'Mark as Redeemed';
    } else if (status === 'redeemed') {
        statusEl.textContent = 'ALREADY REDEEMED';
        statusEl.className = 'result-status status-redeemed';
        detailsEl.innerHTML = `
            <div class="result-detail"><span>Prize:</span> ${info.prize || 'Unknown'}</div>
            <div class="result-detail"><span>Attendee:</span> ${info.attendee || 'Unknown'}</div>
            <div class="result-detail" style="color:#FFD93D;">This ticket has already been used.</div>
        `;
    } else if (status === 'invalid') {
        statusEl.textContent = 'INVALID TICKET';
        statusEl.className = 'result-status status-invalid';
        detailsEl.innerHTML = '<div class="result-detail">This QR code is not a valid ticket.</div>';
    } else {
        statusEl.textContent = 'ERROR';
        statusEl.className = 'result-status status-invalid';
        detailsEl.innerHTML = '<div class="result-detail">Could not verify. Check connection.</div>';
    }
}

// Redeem button
document.getElementById('redeem-btn').addEventListener('click', async () => {
    const btn = document.getElementById('redeem-btn');
    btn.disabled = true;
    btn.textContent = 'Redeeming...';

    try {
        const res = await fetch(`/api/redeem/${encodeURIComponent(lastScannedToken)}`, {
            method: 'POST',
        });
        const data = await res.json();

        if (data.success) {
            btn.textContent = 'Redeemed!';
            btn.style.background = '#4ECDC4';
            document.getElementById('scan-overlay').className = 'scan-overlay';
        } else {
            btn.textContent = data.message || 'Already redeemed';
            btn.style.background = '#FFD93D';
        }
    } catch (e) {
        btn.textContent = 'Network error — try again';
        btn.disabled = false;
    }
});

// Scan again
document.getElementById('scan-again-btn').addEventListener('click', () => {
    scanning = true;
    lastScannedToken = null;
    document.getElementById('result-panel').classList.remove('show');
    document.getElementById('scan-overlay').className = 'scan-overlay';
    document.getElementById('status-line').textContent = 'Point camera at a ticket QR code';
});

setup();
