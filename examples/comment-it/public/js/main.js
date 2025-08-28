import { createMatrixRain, initBlockHeightUpdater, initKonamiCode } from './matrixUI.js';
import { checkExistingWallet, showCreateWallet, showImportWallet, generateNewWallet, copyPrivateKey, validateAndImportWallet, proceedWithWallet, changeWallet } from './walletManager.js';
import { connectWallet, logout, handleAnonymousMode, handleWebSocketMessage, handleAuthenticated, tryRestoreSession } from './authForm.js';
import { initCommentForm, submitComment, addNewComment, showCommentForm } from './commentSection.js';
import { fetchAndDisplayActiveEpisodes, joinEpisode } from './episodeManager.js';

// Global state (moved from index.html script)
window.currentEpisodeId = null;
window.currentSessionToken = null;
window.isAuthenticated = false;
window.currentWallet = null;

// --- CONFIGURABLE BACKEND & PORT ---
function getQueryParam(name) {
  const url = new URL(window.location.href);
  return url.searchParams.get(name);
}

function getBackendPreference() {
  const q = getQueryParam('backend');
  if (q === 'http' || q === 'ws') return q;
  return localStorage.getItem('backend') || 'http';
}

function getPortPreference() {
  const q = getQueryParam('port');
  if (q) return parseInt(q, 10);
  const saved = localStorage.getItem('wsPort');
  return saved ? parseInt(saved, 10) : 8080;
}

let wsBackend = getBackendPreference();
let wsPort = getPortPreference();

// Initialize functions on DOMContentLoaded
document.addEventListener('DOMContentLoaded', () => {
    // Defer showing auth panel until session restore attempts complete
    try { window.deferAuthPanel = true; } catch {}
    // Expose functions to the global scope for onclick attributes
    window.showCreateWallet = showCreateWallet;
    window.showImportWallet = showImportWallet;
    window.generateNewWallet = generateNewWallet;
    window.copyPrivateKey = copyPrivateKey;
    window.validateAndImportWallet = validateAndImportWallet;
    window.proceedWithWallet = proceedWithWallet;
    window.changeWallet = changeWallet;
    window.connectWallet = connectWallet;
    window.logout = logout;
    window.handleAnonymousMode = handleAnonymousMode;
    window.submitComment = submitComment;

    createMatrixRain();
    initKonamiCode();
    initCommentForm();
    checkExistingWallet();
    // Try to restore previous authenticated session (will unset defer flag when done)
    tryRestoreSession();
    // Attempt indexer-based membership restore even without token
    autoMembershipRestore();
    // UI enhancements: indexer status + resume last room if possible
    renderIndexerStatusChip();
    renderResumeLastRoom();

    // Do not force-hide feed; it will be shown when data loads or membership is confirmed
    fetchAndDisplayActiveEpisodes();
    startStatsPolling();

    // Attach event listeners for wallet management
    document.getElementById('createWalletBtn').addEventListener('click', showCreateWallet);
    document.getElementById('importWalletBtn').addEventListener('click', showImportWallet);

    // Manual join existing episode by ID
    const joinInput = document.getElementById('joinEpisodeInput');
    const joinBtn = document.getElementById('joinEpisodeBtn');
    // Top bar join controls
    const topJoinInput = document.getElementById('topJoinEpisodeInput');
    const topJoinBtn = document.getElementById('topJoinEpisodeBtn');
    if (joinBtn && joinInput) {
        joinBtn.addEventListener('click', () => {
            const val = (joinInput.value || '').trim();
            if (!val) return;
            const episodeId = parseInt(val, 10);
            if (!Number.isFinite(episodeId)) {
                alert('Invalid episode id');
                return;
            }
            joinEpisode(String(episodeId));
        });
        joinInput.addEventListener('keydown', (e) => {
            if (e.key === 'Enter') joinBtn.click();
        });
    }
    if (topJoinBtn && topJoinInput) {
        const handler = () => {
            const val = (topJoinInput.value || '').trim();
            if (!val) return;
            const episodeId = parseInt(val, 10);
            if (!Number.isFinite(episodeId)) { alert('Invalid episode id'); return; }
            joinEpisode(String(episodeId));
        };
        topJoinBtn.addEventListener('click', handler);
        topJoinInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') handler(); });
    }

    // Anonymous mode toggle
    const anonMode = document.getElementById('anonMode');
    if (anonMode) {
        anonMode.addEventListener('change', handleAnonymousMode);
    }

    // Initialize organizer peer count (now just a placeholder)
    const orgPeersElement = document.getElementById('organizerPeers');
    if (orgPeersElement) {
        orgPeersElement.textContent = '1'; // Always 1 for the local peer
    }

    // Expose handleWebSocketMessage and handleAuthenticated to global scope for WebSocket and authForm
    window.handleWebSocketMessage = handleWebSocketMessage;
    window.handleAuthenticated = handleAuthenticated;
    window.showCommentForm = showCommentForm;
    window.addNewComment = addNewComment;

    // Initialize backend selector UI
    const backendSelect = document.getElementById('backendSelect');
    if (backendSelect) {
        backendSelect.value = wsBackend;
        backendSelect.addEventListener('change', () => {
            wsBackend = backendSelect.value;
            localStorage.setItem('backend', wsBackend);
            reconnectWebSocket();
        });
    }

    reconnectWebSocket();

    // Wire top bar logout and auth indicator
    try {
        const logoutBtn = document.getElementById('topLogoutBtn');
        if (logoutBtn) logoutBtn.onclick = logout;
        window.updateTopBarAuth = (authed) => {
            const ind = document.getElementById('topAuthIndicator');
            const btn = document.getElementById('topLogoutBtn');
            if (ind) ind.textContent = authed ? 'Auth: authenticated' : 'Auth: guest';
            if (btn) btn.style.display = authed ? 'inline-block' : 'none';
        };
        window.updateTopBarAuth(false);
    } catch {}

});

function getIndexerUrl() {
    try { return localStorage.getItem('indexerUrl') || 'http://127.0.0.1:8090'; } catch { return 'http://127.0.0.1:8090'; }
}

function renderIndexerStatusChip() {
    let chip = document.getElementById('indexerStatusChip');
    if (!chip) {
        chip = document.createElement('div');
        chip.id = 'indexerStatusChip';
        chip.style.cssText = 'position:fixed;top:8px;right:8px;background:#082e2e;color:#a6ffef;border:1px solid #15e6d1;border-radius:8px;padding:6px 10px;font:12px monospace;z-index:9999;display:flex;gap:8px;align-items:center;';
        document.body.appendChild(chip);
    }
    async function refresh() {
        const base = getIndexerUrl();
        let status = 'offline';
        try {
            const r = await fetch(base + '/index/health', { cache: 'no-store' });
            status = r.ok ? 'online' : 'offline';
        } catch {}
        chip.textContent = `kdapp-indexer: ${status} @ ${base}`;
    }
    refresh();
    setInterval(refresh, 7000);
}

async function renderResumeLastRoom() {
    try {
        const last = localStorage.getItem('last_episode_id');
        const pub = localStorage.getItem('participant_pubkey');
        if (!last) return; // Nothing to resume
        const episodeId = parseInt(last, 10);
        if (!Number.isFinite(episodeId)) return;

        // Query indexer membership if we have a pubkey; otherwise still offer resume to just load feed
        let member = false;
        if (pub) {
            try {
                const base = getIndexerUrl();
                const r = await fetch(`${base}/index/me/${episodeId}?pubkey=${pub}`);
                if (r.ok) { const j = await r.json(); member = !!j.member; }
            } catch {}
        }

        let bar = document.getElementById('resumeLastRoomBar');
        if (!bar) {
            bar = document.createElement('div');
            bar.id = 'resumeLastRoomBar';
            bar.style.cssText = 'position:fixed;bottom:8px;left:50%;transform:translateX(-50%);background:#062424;color:#a6ffef;border:1px solid #15e6d1;border-radius:10px;padding:8px 12px;font:12px monospace;z-index:9999;display:flex;gap:10px;align-items:center;';
            document.body.appendChild(bar);
        }
        bar.innerHTML = '';
        const span = document.createElement('span');
        span.textContent = member ? `Resume room ${episodeId} (authenticated)` : `Resume room ${episodeId}`;
        const btn = document.createElement('button');
        btn.textContent = 'Resume';
        btn.style.cssText = 'padding:4px 8px;border:1px solid #15e6d1;background:transparent;color:#15e6d1;cursor:pointer;border-radius:6px;';
        btn.onclick = () => {
            // Programmatically join the room and let existing flows load feed/auth state
            joinEpisode(String(episodeId));
            // Hide bar after action
            bar.style.display = 'none';
        };
        const close = document.createElement('button');
        close.textContent = 'Dismiss';
        close.style.cssText = 'padding:4px 8px;border:1px solid #0a4848;background:transparent;color:#0a9d9d;cursor:pointer;border-radius:6px;';
        close.onclick = () => { bar.style.display = 'none'; };
        bar.appendChild(span);
        bar.appendChild(btn);
        bar.appendChild(close);
    } catch {}
}

async function autoMembershipRestore() {
    try {
        let last = localStorage.getItem('last_episode_id');
        if (!last) return;
        let episodeId = parseInt(last, 10);
        if (!Number.isFinite(episodeId)) {
            // Fallback: read from current UI if present
            try {
                const t = (document.getElementById('episodeId')?.textContent || '').trim();
                const n = parseInt(t, 10);
                if (Number.isFinite(n)) { episodeId = n; }
            } catch {}
        }
        if (!Number.isFinite(episodeId)) return;
        let pub = localStorage.getItem('participant_pubkey');
        if (!pub) {
            // Try to fetch existing wallet pubkey without creating anything
            try {
                const r = await fetch('/wallet-participant', { cache: 'no-store' });
                if (r.ok) {
                    const j = await r.json();
                    if (j && j.public_key && j.public_key !== 'none' && !j.error) {
                        pub = j.public_key;
                        localStorage.setItem('participant_pubkey', pub);
                    }
                }
            } catch {}
        }
        if (!pub) return;

        // Check indexer membership
        const base = getIndexerUrl();
        try {
            const r = await fetch(`${base}/index/me/${episodeId}?pubkey=${pub}`);
            if (r.ok) {
                const j = await r.json();
                if (j && j.member) {
                    // Ensure episode id is reflected in UI and global state
                    try { document.getElementById('episodeId').textContent = episodeId; } catch {}
                    try { window.currentEpisodeId = episodeId; } catch {}
                    // Auto-auth UI
                    window.indexerMember = true;
                    handleAuthenticated('pure_p2p');
                }
            }
        } catch {}
    } catch {}
}

function currentWsUrl() {
    const base = `ws://localhost:${wsPort}`;
    return wsBackend === 'http' ? `${base}/ws` : base;
}

function setWsStatus(state, url) {
    const el = document.getElementById('wsStatus');
    if (!el) return;
    el.classList.remove('ws-connecting', 'ws-ok', 'ws-bad');
    el.title = url || '';
    if (state === 'ok') { el.textContent = 'connected'; el.classList.add('ws-ok'); }
    else if (state === 'connecting') { el.textContent = 'connecting'; el.classList.add('ws-connecting'); }
    else { el.textContent = 'disconnected'; el.classList.add('ws-bad'); }
}

function reconnectWebSocket() {
    try {
        if (window.commandWebSocket) {
            try { window.commandWebSocket.close(); } catch {}
        }
        const primaryUrl = currentWsUrl();
        console.log('Connecting WebSocket to', primaryUrl);
        setWsStatus('connecting', primaryUrl);

        let triedFallback = false;

        const openWithUrl = (url) => {
            try {
                window.commandWebSocket = new WebSocket(url);
            } catch (e) {
                console.error('WebSocket constructor failed for', url, e);
                setWsStatus('bad', url);
                return;
            }

            window.commandWebSocket.onopen = () => {
                console.log('✅ Command WebSocket connected to backend');
                setWsStatus('ok', url);
            };

            window.commandWebSocket.onmessage = (event) => {
                try {
                    const message = JSON.parse(event.data);
                    // Central dispatcher: only this handler processes messages
                    if (message.type === 'new_comment') {
                        addNewComment(message);
                    } else if (message.type === 'authentication_successful') {
                        handleAuthenticated(message.session_token);
                    } else if (message.type === 'authentication_failed') {
                        handleAuthenticationFailed(message.error);
                    } else if (message.type === 'session_revoked') {
                        handleSessionRevoked();
                    } else if (message.type === 'episode_rolled_back') {
                        console.warn('Episode rolled back:', message);
                    } else if (message.status === 'submitted') {
                        console.log(`Transaction submitted! TxId: ${message.tx_id}`);
                    } else if (message.status === 'error') {
                        console.error(`Backend error: ${message.message}`);
                    } else {
                        // Pass remaining auth-related messages to authForm handler
                        handleWebSocketMessage(message);
                    }
                } catch (e) {
                    console.error('Error parsing WebSocket message:', e);
                }
            };

            window.commandWebSocket.onerror = (error) => {
                console.error('❌ Command WebSocket error:', error);
                // One-time fallback between base and /ws if wrong backend selected
                if (!triedFallback) {
                    triedFallback = true;
                    const base = `ws://localhost:${wsPort}`;
                    const alt = url.endsWith('/ws') ? base : `${base}/ws`;
                    console.log('Trying fallback WebSocket URL', alt);
                    setTimeout(() => openWithUrl(alt), 200);
                    return;
                }
                setWsStatus('bad', url);
            };

            window.commandWebSocket.onclose = () => {
                console.log('❌ Command WebSocket disconnected from backend');
                setWsStatus('bad', url);
                setTimeout(() => {
                    console.log('Attempting to reconnect Command WebSocket...');
                    reconnectWebSocket();
                }, 3000);
            };
        };

        openWithUrl(primaryUrl);
    } catch (e) {
        console.error('Failed to (re)connect WebSocket', e);
        setWsStatus('bad');
    }
}

window.reconnectWebSocket = reconnectWebSocket;

function startStatsPolling() {
    async function fetchStats() {
        try {
            const res = await fetch('/stats');
            if (!res.ok) throw new Error(`HTTP ${res.status}`);
            const s = await res.json();
            // Organizer peers
            const op = document.getElementById('organizerPeers');
            if (op && typeof s.organizer_peers !== 'undefined') op.textContent = s.organizer_peers.toLocaleString();
            // Auth episodes
            const ae = document.getElementById('authEpisodes');
            if (ae && typeof s.auth_episodes !== 'undefined') ae.textContent = s.auth_episodes.toLocaleString();
            // Comment episodes (total comments)
            const ce = document.getElementById('commentEpisodes');
            if (ce && typeof s.comment_episodes !== 'undefined') ce.textContent = s.comment_episodes.toLocaleString();
            // DAA score
            const ds = document.getElementById('daaScore');
            if (ds && typeof s.daa_score !== 'undefined' && s.daa_score !== null) ds.textContent = Number(s.daa_score).toLocaleString();
            // Block height display in status bar
            const bh = document.getElementById('blockHeight');
            if (bh && typeof s.block_height !== 'undefined' && s.block_height !== null) bh.textContent = Number(s.block_height).toLocaleString();

            // Merge kdapp-indexer metrics if available (persistence-aware)
            try {
                const idxUrl = (localStorage.getItem('indexerUrl') || 'http://127.0.0.1:8090') + '/index/metrics';
                const ir = await fetch(idxUrl);
                if (ir.ok) {
                    const im = await ir.json();
                    // Map: episodes -> authEpisodes, comments -> commentEpisodes
                    if (ae && typeof im.episodes === 'number') ae.textContent = im.episodes.toLocaleString();
                    if (ce && typeof im.comments === 'number') ce.textContent = im.comments.toLocaleString();
                }
            } catch {}
        } catch (e) {
            console.warn('Failed to fetch /stats', e);
        }
    }
    fetchStats();
    setInterval(fetchStats, 5000);
}
