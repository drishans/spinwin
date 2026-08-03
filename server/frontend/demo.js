// Demo mode — lets the frontend run with no backend at all.
//
// Two hosts use this:
//   * A static host (GitHub Pages) where /api/* does not exist.
//   * The real server started with SPINWIN_DEMO=1, which reports demo:true from
//     /api/config so the live URL can stay up between events without consuming
//     real stock, writing tickets, or sending email.
//
// Everything below is a client-side stand-in. Prize selection mirrors the
// server's weighted-by-remaining draw and its landing-angle math so the wheel
// animation looks identical to the real thing — but nothing is signed and
// nothing is stored.

const DEMO_PRIZES = [
    { id: 1, name: 'Necklace',     image_url: 'necklace.jpg',    total_qty: 100, remaining: 100 },
    { id: 2, name: 'Ring',         image_url: 'ring.jpg',        total_qty: 200, remaining: 200 },
    { id: 3, name: 'Jewelry Set',  image_url: 'jewelry_set.jpg', total_qty: 30,  remaining: 30 },
    { id: 4, name: 'Earring',      image_url: 'earring.jpg',     total_qty: 100, remaining: 100 },
    { id: 5, name: 'Bangles',      image_url: 'bangles2.jpg',    total_qty: 50,  remaining: 50 },
    { id: 6, name: 'Mystery Prize', image_url: 'mystery.svg',    total_qty: 20,  remaining: 20 },
];

const Demo = (() => {
    let active = false;
    // Stock is per-session only: a reload restores the full wheel.
    let prizes = DEMO_PRIZES.map(p => ({ ...p }));

    function randRange(min, max) {
        return Math.random() * (max - min) + min;
    }

    function randInt(min, max) {
        return Math.floor(Math.random() * (max - min)) + min;
    }

    // Mirrors spin() in server/src/main.rs: weighted draw over in-stock prizes,
    // falling back to Mystery Prize once everything else is gone.
    function pickPrize() {
        const inStock = prizes.filter(p => p.remaining > 0);
        if (inStock.length === 0) {
            return prizes.find(p => p.name === 'Mystery Prize') || prizes[0];
        }
        const total = inStock.reduce((sum, p) => sum + p.remaining, 0);
        const roll = randInt(0, total);
        let cumulative = 0;
        for (const p of inStock) {
            cumulative += p.remaining;
            if (roll < cumulative) return p;
        }
        return inStock[0];
    }

    // Same landing-angle calculation the server uses, so the pointer stops on
    // the drawn segment rather than somewhere near it.
    function landingAngle(prize) {
        const idx = prizes.findIndex(p => p.id === prize.id);
        const segmentSize = 360 / prizes.length;
        const segmentStart = (idx < 0 ? 0 : idx) * segmentSize;
        const withinSegment = randRange(0.2, 0.8) * segmentSize;
        const fullRotations = randInt(5, 8) * 360;
        return fullRotations + (360 - (segmentStart + withinSegment));
    }

    function nameFromEmail(email) {
        const local = email.split('@')[0].replace(/[._-]+/g, ' ').trim();
        if (!local) return 'Guest';
        return local.split(/\s+/)
            .map(w => w.charAt(0).toUpperCase() + w.slice(1))
            .join(' ');
    }

    // Deliberately NOT shaped like a real Ed25519 ticket. A demo QR must never
    // be mistakable for a signed one — the scanner should reject it outright.
    function fakeToken(email, prizeName) {
        const id = Math.random().toString(36).slice(2, 10);
        return `DEMO-TICKET.not-a-real-ticket.${id}.${encodeURIComponent(prizeName)}`;
    }

    return {
        get active() { return active; },

        // Decide whether this page runs against a live backend.
        //   ?demo=1 forces demo, ?demo=0 forces live (useful for testing).
        // Otherwise ask the server; anything other than an explicit demo:false
        // answer (404, network error, static host) means demo.
        async detect() {
            const param = new URLSearchParams(location.search).get('demo');
            if (param === '1') { active = true; return true; }
            if (param === '0') { active = false; return false; }

            try {
                const res = await fetch('api/config', { cache: 'no-store' });
                if (!res.ok) { active = true; return true; }
                const cfg = await res.json();
                active = cfg.demo === true;
            } catch (e) {
                active = true;
            }
            return active;
        },

        prizes() {
            return prizes.map(p => ({ ...p }));
        },

        checkEmail(email) {
            return {
                already_played: false,
                not_registered: false,
                attendee_name: nameFromEmail(email),
            };
        },

        spin(email) {
            const prize = pickPrize();
            if (prize.remaining > 0) prize.remaining -= 1;
            const attendee = nameFromEmail(email);
            return {
                prize: { ...prize },
                angle: landingAngle(prize),
                ticket_id: 'demo-' + Math.random().toString(36).slice(2, 10),
                qr_data: fakeToken(email, prize.name),
                prize_name: prize.name,
                attendee_name: attendee,
            };
        },

        reset() {
            prizes = DEMO_PRIZES.map(p => ({ ...p }));
        },

        // Banner shown on every demo page. Anyone arriving from the blog link
        // needs to know up front that no real prize is being handed out.
        showBanner(text) {
            if (document.getElementById('demo-banner')) return;
            const el = document.createElement('div');
            el.id = 'demo-banner';
            el.className = 'demo-banner';
            el.innerHTML = text || 'Demo preview — spin all you like, but prizes and tickets aren\'t real. The live giveaway opens in October.';
            document.body.prepend(el);
            document.body.classList.add('has-demo-banner');
        },
    };
})();
