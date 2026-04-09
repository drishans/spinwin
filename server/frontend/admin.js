async function loadAll() {
    const [statsRes, ticketsRes] = await Promise.all([
        fetch('/api/admin/stats'),
        fetch('/api/admin/tickets'),
    ]);
    const stats = await statsRes.json();
    const ticketData = await ticketsRes.json();

    // Stats bar
    const totalClaimed = stats.prizes.reduce((s, p) => s + p.claimed, 0);
    const totalStock = stats.prizes.reduce((s, p) => s + p.total_qty, 0);
    document.getElementById('stats-bar').innerHTML = `
        <div class="stat"><div class="number">${stats.total_tickets}</div><div class="label">Tickets Issued</div></div>
        <div class="stat"><div class="number">${stats.total_redeemed}</div><div class="label">Redeemed</div></div>
        <div class="stat"><div class="number">${totalStock - totalClaimed}</div><div class="label">Stock Left</div></div>
        <div class="stat"><div class="number">${stats.registered_emails}</div><div class="label">Registered</div></div>
    `;

    // Prize table
    document.getElementById('prize-table').innerHTML = stats.prizes.map(p => {
        const pct = p.total_qty > 0 ? (p.remaining / p.total_qty * 100) : 0;
        const low = pct < 20 ? 'low' : '';
        return `<tr>
            <td>${p.name}</td>
            <td>
                <span class="stock-bar"><span class="stock-bar-fill ${low}" style="width:${pct}%"></span></span>
                ${p.remaining} / ${p.total_qty}
            </td>
            <td>${p.claimed}</td>
            <td>
                <input class="stock-input" id="stock-${p.id}" type="number" value="${p.total_qty}" min="0" title="Total inventory">
                <button class="stock-btn" onclick="updateStock(${p.id})">Set Total</button>
                <span class="msg" id="msg-${p.id}"></span>
            </td>
        </tr>`;
    }).join('');

    // Ticket table
    document.getElementById('ticket-table').innerHTML = ticketData.tickets.map(t => {
        const badge = t.redeemed
            ? '<span class="badge badge-yes">Yes</span>'
            : '<span class="badge badge-no">No</span>';
        const time = t.created_at ? new Date(t.created_at + 'Z').toLocaleString() : '';
        return `<tr>
            <td>${t.name}</td>
            <td>${t.email}</td>
            <td>${t.prize}</td>
            <td>${badge}</td>
            <td>${time}</td>
        </tr>`;
    }).join('');
}

async function updateStock(prizeId) {
    const input = document.getElementById(`stock-${prizeId}`);
    const msg = document.getElementById(`msg-${prizeId}`);
    const val = parseInt(input.value);
    if (isNaN(val) || val < 0) {
        msg.textContent = 'Invalid';
        return;
    }
    const res = await fetch(`/api/admin/prizes/${prizeId}/stock`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ total_qty: val }),
    });
    if (res.ok) {
        msg.textContent = 'Updated';
        setTimeout(() => { msg.textContent = ''; loadAll(); }, 1000);
    } else {
        msg.textContent = 'Failed';
    }
}

loadAll();
// Auto-refresh every 30 seconds
setInterval(loadAll, 30000);
