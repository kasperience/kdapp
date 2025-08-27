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
    // Try to restore previous authenticated session
    tryRestoreSession();
    fetchAndDisplayActiveEpisodes();
    startStatsPolling();

    // Attach event listeners for wallet management
    document.getElementById('createWalletBtn').addEventListener('click', showCreateWallet);
    document.getElementById('importWalletBtn').addEventListener('click', showImportWallet);

    // Manual join existing episode by ID
    const joinInput = document.getElementById('joinEpisodeInput');
    const joinBtn = document.getElementById('joinEpisodeBtn');
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

});

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
        } catch (e) {
            console.warn('Failed to fetch /stats', e);
        }
    }
    fetchStats();
    setInterval(fetchStats, 5000);
}
