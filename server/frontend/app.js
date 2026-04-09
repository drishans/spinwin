const PRIZE_COLORS = [
    '#FF6B6B', '#4ECDC4', '#FFD93D', '#6C5CE7', '#A8E6CF', '#FF8C42'
];
const PRIZE_COLORS_DARK = [
    '#cc5555', '#3ea89e', '#ccad31', '#5649b8', '#86b8a5', '#cc7035'
];

let prizes = [];
let spinning = false;
let spinResult = null;
let verifiedEmail = null;
let verifiedName = null;

const canvas = document.getElementById('wheel-canvas');
const ctx = canvas.getContext('2d');
const centerX = canvas.width / 2;
const centerY = canvas.height / 2;
const radius = canvas.width / 2 - 20;

// Load prizes
async function loadPrizes() {
    const res = await fetch('/api/prizes');
    prizes = await res.json();
    drawWheel(0);
}

function getSegments() {
    const available = prizes.filter(p => p.remaining > 0);
    if (available.length === 0) return [];

    // Equal size segments — probability is handled server-side
    const angle = 360 / available.length;
    return available.map((p, i) => ({
        prize: p,
        startAngle: i * angle,
        sweepAngle: angle,
    }));
}

function drawWheel(rotation) {
    ctx.clearRect(0, 0, canvas.width, canvas.height);

    const segments = getSegments();
    if (segments.length === 0) {
        ctx.fillStyle = '#666';
        ctx.font = '24px sans-serif';
        ctx.textAlign = 'center';
        ctx.fillText('All prizes claimed!', centerX, centerY);
        return;
    }

    segments.forEach((seg, i) => {
        const startRad = (seg.startAngle + rotation - 90) * Math.PI / 180;
        const endRad = (seg.startAngle + seg.sweepAngle + rotation - 90) * Math.PI / 180;
        const colorIdx = prizes.findIndex(p => p.id === seg.prize.id) % PRIZE_COLORS.length;

        // Draw segment
        ctx.beginPath();
        ctx.moveTo(centerX, centerY);
        ctx.arc(centerX, centerY, radius, startRad, endRad);
        ctx.closePath();
        ctx.fillStyle = PRIZE_COLORS[colorIdx];
        ctx.fill();
        ctx.strokeStyle = '#1a0a2e';
        ctx.lineWidth = 3;
        ctx.stroke();

        // Draw text
        const midAngle = startRad + (endRad - startRad) / 2;
        ctx.save();
        ctx.translate(centerX, centerY);
        ctx.rotate(midAngle);
        ctx.fillStyle = '#1a0a2e';
        ctx.font = 'bold 28px sans-serif';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';

        // Position text at 60% of radius
        const textRadius = radius * 0.6;
        ctx.fillText(seg.prize.name, textRadius, 0);
        ctx.restore();
    });

    // Center circle
    ctx.beginPath();
    ctx.arc(centerX, centerY, 40, 0, Math.PI * 2);
    ctx.fillStyle = '#1a0a2e';
    ctx.fill();
    ctx.strokeStyle = '#f9d423';
    ctx.lineWidth = 3;
    ctx.stroke();
}

// Spin animation
function animateSpin(targetAngle, duration, callback) {
    const startTime = performance.now();
    const startRotation = 0;

    function easeOutCubic(t) {
        return 1 - Math.pow(1 - t, 3);
    }

    function animate(now) {
        const elapsed = now - startTime;
        const progress = Math.min(elapsed / duration, 1);
        const eased = easeOutCubic(progress);
        const currentAngle = startRotation + targetAngle * eased;

        drawWheel(currentAngle % 360);

        if (progress < 1) {
            requestAnimationFrame(animate);
        } else {
            callback();
        }
    }

    requestAnimationFrame(animate);
}

// Confetti
function launchConfetti() {
    const confettiCanvas = document.getElementById('confetti');
    const cCtx = confettiCanvas.getContext('2d');
    confettiCanvas.width = window.innerWidth;
    confettiCanvas.height = window.innerHeight;

    const particles = [];
    const colors = ['#f9d423', '#ff4e50', '#4ECDC4', '#FFD93D', '#FF6B6B', '#6C5CE7'];

    for (let i = 0; i < 150; i++) {
        particles.push({
            x: Math.random() * confettiCanvas.width,
            y: Math.random() * confettiCanvas.height - confettiCanvas.height,
            w: Math.random() * 10 + 5,
            h: Math.random() * 6 + 3,
            color: colors[Math.floor(Math.random() * colors.length)],
            vx: (Math.random() - 0.5) * 4,
            vy: Math.random() * 3 + 2,
            rot: Math.random() * 360,
            rotV: (Math.random() - 0.5) * 10,
        });
    }

    let frame = 0;
    function drawConfetti() {
        cCtx.clearRect(0, 0, confettiCanvas.width, confettiCanvas.height);
        particles.forEach(p => {
            p.x += p.vx;
            p.y += p.vy;
            p.rot += p.rotV;
            p.vy += 0.05;

            cCtx.save();
            cCtx.translate(p.x, p.y);
            cCtx.rotate(p.rot * Math.PI / 180);
            cCtx.fillStyle = p.color;
            cCtx.fillRect(-p.w / 2, -p.h / 2, p.w, p.h);
            cCtx.restore();
        });

        frame++;
        if (frame < 180) requestAnimationFrame(drawConfetti);
        else cCtx.clearRect(0, 0, confettiCanvas.width, confettiCanvas.height);
    }
    requestAnimationFrame(drawConfetti);
}

// Email gate — verify email before allowing spin
document.getElementById('gate-btn').addEventListener('click', async () => {
    const email = document.getElementById('gate-email').value.trim();
    const errorEl = document.getElementById('gate-error');

    if (!email) {
        errorEl.textContent = 'Please enter your email';
        return;
    }
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) {
        errorEl.textContent = 'Please enter a valid email address';
        return;
    }

    document.getElementById('gate-btn').disabled = true;
    errorEl.textContent = '';

    try {
        const res = await fetch(`/api/check-email/${encodeURIComponent(email)}`);
        const data = await res.json();

        if (data.not_registered) {
            errorEl.textContent = 'This email is not registered for the event';
            document.getElementById('gate-btn').disabled = false;
            return;
        }

        if (data.already_played) {
            // Recover existing ticket — show it directly
            showTicket(data.ticket, true);
            return;
        }

        // Email is valid and unused — show spin button
        verifiedEmail = email;
        verifiedName = data.attendee_name || email;
        document.getElementById('email-gate').style.display = 'none';
        document.getElementById('spin-btn').style.display = 'block';
    } catch (e) {
        errorEl.textContent = 'Network error — please try again';
        document.getElementById('gate-btn').disabled = false;
    }
});

// Allow Enter key on email input
document.getElementById('gate-email').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') document.getElementById('gate-btn').click();
});

// Spin button — now sends email to server
document.getElementById('spin-btn').addEventListener('click', async () => {
    if (spinning || !verifiedEmail) return;
    spinning = true;
    document.getElementById('spin-btn').disabled = true;

    try {
        const res = await fetch('/api/spin', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ email: verifiedEmail }),
        });
        if (!res.ok) {
            const err = await res.json();
            alert(err.error || 'Failed to spin');
            spinning = false;
            document.getElementById('spin-btn').disabled = false;
            return;
        }

        spinResult = await res.json();

        animateSpin(spinResult.angle, 4000, () => {
            spinning = false;
            launchConfetti();
            showTicket(spinResult);
        });
    } catch (e) {
        alert('Network error — please try again');
        spinning = false;
        document.getElementById('spin-btn').disabled = false;
    }
});

function showTicket(data, skipFront) {
    // Hide everything else
    document.querySelector('header').style.display = 'none';
    document.querySelector('.wheel-container').style.display = 'none';
    document.getElementById('spin-btn').style.display = 'none';
    document.getElementById('email-gate').style.display = 'none';
    document.getElementById('confetti').style.display = 'none';

    // Populate front (prize reveal)
    document.getElementById('winner-name').textContent = data.attendee_name || verifiedName || '';
    document.getElementById('prize-name-front').textContent = data.prize_name;
    const prizeImg = document.getElementById('prize-img');
    if (data.prize && data.prize.image_url) {
        prizeImg.src = `/assets/${data.prize.image_url}`;
        prizeImg.alt = data.prize_name;
    } else {
        prizeImg.style.display = 'none';
    }

    // Populate back (ticket)
    document.getElementById('ticket-prize').textContent = data.prize_name;
    document.getElementById('ticket-attendee').textContent = data.attendee_name;

    // Resend button
    const resendBtn = document.getElementById('resend-btn');
    resendBtn.style.display = 'inline-block';
    resendBtn.onclick = async () => {
        const email = verifiedEmail || document.getElementById('gate-email').value.trim();
        if (!email) return;
        resendBtn.disabled = true;
        const msgEl = document.getElementById('resend-msg');
        try {
            const res = await fetch(`/api/resend/${encodeURIComponent(email)}`, { method: 'POST' });
            msgEl.textContent = res.ok ? 'Email sent! Check your inbox.' : 'Failed to resend — please try again.';
        } catch (e) {
            msgEl.textContent = 'Network error — please try again.';
        }
        resendBtn.disabled = false;
    };

    // Generate QR code
    const qrContainer = document.getElementById('ticket-qr');
    qrContainer.innerHTML = '';
    if (typeof QRCode !== 'undefined' && QRCode.toCanvas) {
        const qrCanvas = document.createElement('canvas');
        QRCode.toCanvas(qrCanvas, data.qr_data, {
            width: 250, margin: 1,
            color: { dark: '#1a0a2e', light: '#ffffff' },
        }, function(err) {
            if (err) {
                qrContainer.innerHTML = '<p style="color:#ff6b6b;">QR failed. Ticket: ' + data.ticket_id + '</p>';
            } else {
                qrContainer.appendChild(qrCanvas);
            }
        });
    } else {
        qrContainer.innerHTML = '<p style="color:#ff6b6b;">QR library failed to load.<br>Ticket ID: ' + data.ticket_id + '</p>';
    }

    // Show the card
    const flipCard = document.getElementById('flip-card');
    const ticketView = document.getElementById('ticket-view');
    ticketView.style.display = 'block';

    // If recovering an existing ticket, skip to the back directly
    if (skipFront) {
        flipCard.classList.add('flipped');
    } else {
        flipCard.classList.remove('flipped');
    }

    // Wire up flip button
    document.getElementById('flip-btn').onclick = () => {
        flipCard.classList.add('flipped');
    };
}

// Init
loadPrizes();
