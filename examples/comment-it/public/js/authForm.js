import { resilientFetch, typewriterEffect, truncateKaspaAddress } from './utils.js';
import { showCommentForm, handleNewComment, loadFeedForEpisode } from './commentSection.js';
import { currentWallet, showAuthPanel, showFundingInfo } from './walletManager.js'; // Added comment to force refresh

// Use window.currentEpisodeId as the single source of truth across modules
export let currentSessionToken = null;
export let isAuthenticated = false;

// Getter function for currentEpisodeId that always reads from window
export function getCurrentEpisodeId() {
    return window.currentEpisodeId;
}

// Setter function for currentEpisodeId that updates both module and window
export function setCurrentEpisodeId(episodeId) {
    window.currentEpisodeId = episodeId;
}
let isProcessingChallenge = false; // Prevent duplicate challenge processing
let isProcessingLogout = false; // Prevent duplicate logout processing
let isProcessingEpisodeCreation = false; // Prevent duplicate episode creation

export let webSocket = null;

// Real API functions
export async function connectWallet() {
    if (!currentWallet) {
        alert('No wallet available. Please create or import a wallet first.');
        return;
    }
    
    // Prevent duplicate episode creation
    if (isProcessingEpisodeCreation) {
        console.log('🔄 Episode creation already in progress - ignoring duplicate');
        return;
    }
    
    const button = event.target;
    const originalText = button.textContent;
    button.textContent = '[ CONNECTING TO KASPA... ]';
    button.disabled = true;
    isProcessingEpisodeCreation = true;
    
    // Hide logout button at start of authentication flow
    const logoutBtn = document.getElementById('logoutButton');
    if (logoutBtn) {
        logoutBtn.style.display = 'none';
        console.log('🔍 DEBUG: Logout button hidden at auth start');
    }
    
    try {
        // Step 1: Get wallet public key if needed
        let walletData;
        if (currentWallet.publicKey === 'from_file' || !currentWallet.publicKey) {
            const walletResponse = await resilientFetch('/wallet-participant');
            walletData = await walletResponse.json();
            
            if (walletData.error) {
                throw new Error(walletData.error);
            }
            
            currentWallet.publicKey = walletData.public_key;
            
            if (walletData.was_created || walletData.needs_funding) {
                button.textContent = '[ WALLET NEEDS FUNDING ]';
                showFundingInfo(currentWallet.kaspaAddress);
                button.disabled = false;
                return;
            }
        } else {
            walletData = {
                public_key: currentWallet.publicKey,
                kaspa_address: currentWallet.kaspaAddress
            };
        }
        
        // Step 2: Start authentication episode or join existing one
        const authBody = {
            public_key: walletData.public_key
        };
        
        // If currentEpisodeId is already set (from joining existing episode), include it
        const currentEpisodeId = getCurrentEpisodeId();
        if (currentEpisodeId) {
            authBody.episode_id = currentEpisodeId;
            console.log(`🎯 Authenticating for existing episode: ${currentEpisodeId}`);
        } else {
            console.log(`🆕 Creating new authentication episode`);
        }
        
        const authResponse = await resilientFetch('/auth/start', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(authBody)
        });
        
        const authData = await authResponse.json();
        
        if (authData.status === 'submitted_to_blockchain' || authData.status === 'transaction_submission_failed' || authData.status === 'joined_existing_episode') {
            setCurrentEpisodeId(authData.episode_id);
            
            // Update UI
            const episodeId = getCurrentEpisodeId();
            document.getElementById('episodeId').textContent = episodeId;
            document.getElementById('authEpisodeDisplay').textContent = episodeId;
            
            if (authData.status === 'submitted_to_blockchain') {
                button.textContent = '[ WAITING FOR CHALLENGE... ]';
                typewriterEffect(`CONNECTED TO KASPA NETWORK. AUTHENTICATING...`, button.parentElement);
            } else if (authData.status === 'joined_existing_episode') {
                button.textContent = '[ REQUESTING CHALLENGE... ]';
                typewriterEffect(`JOINED COMMENT ROOM ${getCurrentEpisodeId()}. REQUESTING CHALLENGE...`, button.parentElement);
                // For existing episodes, connect WebSocket first then request challenge
                connectWebSocket();
                // Small delay to ensure WebSocket is connected
                setTimeout(() => {
                    requestChallengeAfterEpisodeCreation();
                }, 500);
            } else {
                button.textContent = '[ RETRYING CONNECTION... ]';
                typewriterEffect(`INITIAL SUBMISSION FAILED. RETRYING VIA WEBSOCKET...`, button.parentElement);
            }
            
            // Hide logout button during challenge wait
            const logoutBtn = document.getElementById('logoutButton');
            if (logoutBtn) {
                logoutBtn.style.display = 'none';
                console.log('🔍 DEBUG: Logout button hidden during challenge wait');
            }
            
            // Load persistent feed from indexer and connect WebSocket for real-time updates
            loadFeedForEpisode(getCurrentEpisodeId());
            // Connect WebSocket for real-time updates (even if initial submission failed)
            connectWebSocket();
        } else {
            throw new Error('Login failed: ' + authData.status);
        }
        
    } catch (error) {
        console.error('Authentication failed:', error);
        
        if (error.message.includes('WALLET_NEEDS_FUNDING')) {
            button.textContent = '[ WALLET NEEDS FUNDING ]';
            button.style.background = 'var(--warning)';
            button.style.borderColor = 'var(--warning)';
            typewriterEffect(`WALLET NEEDS FUNDING! Visit https://faucet.kaspanet.io/ and fund: ${currentWallet.kaspaAddress}`, button.parentElement);
        } else {
            button.textContent = '[ ERROR - TRY AGAIN ]';
            typewriterEffect(`ERROR: ${error.message}`, button.parentElement);
        }
        
        button.disabled = false;
        isProcessingEpisodeCreation = false; // Reset state lock on error
    }
}

// WebSocket connection for real-time updates
export function connectWebSocket() {
    // Reuse the shared command WebSocket managed by main.js if available
    try {
        if (window.commandWebSocket) {
            const ws = window.commandWebSocket;
            // Do not attach another message listener here; main.js is the central dispatcher
            if (ws.readyState === WebSocket.OPEN) {
                console.log('✅ WebSocket connected (shared)');
            }
            return;
        }
    } catch {}

    // Fallback: only create a dedicated socket if the global one is not present
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = `${protocol}//${window.location.host}/ws`;
    webSocket = new WebSocket(wsUrl);

    webSocket.onopen = () => {
        console.log('✅ WebSocket connected');
    };
    webSocket.onmessage = (event) => {
        try {
            const message = JSON.parse(event.data);
            handleWebSocketMessage(message);
        } catch (error) {
            console.error('WebSocket message parsing error:', error);
        }
    };
    webSocket.onclose = () => {
        console.log('❌ WebSocket disconnected');
        // If main.js is not managing the socket, attempt local reconnect
        if (!window.commandWebSocket) {
            setTimeout(connectWebSocket, 3000);
        }
    };
    webSocket.onerror = (error) => {
        console.error('WebSocket error:', error);
    };
}

// Handle real-time WebSocket messages
export function handleWebSocketMessage(message) {
    console.log('📨 WebSocket message:', message);
    
    switch (message.type) {
        case 'episode_created':
            // Only respond to episode_created if we initiated creation in this tab
            if (!isProcessingEpisodeCreation) {
                console.log('ℹ️ Episode_created received (rehydrate or external); ignoring auto-challenge');
                // Optionally update current episode id if not set
                if (!getCurrentEpisodeId()) setCurrentEpisodeId(message.episode_id);
                return;
            }
            console.log('🎯 Episode created (local flow), requesting challenge...');
            setCurrentEpisodeId(message.episode_id);
            isProcessingEpisodeCreation = false;
            if (!isAuthenticated) {
                requestChallengeAfterEpisodeCreation();
            }
            break;
            
        case 'challenge_issued':
            if (message.episode_id === getCurrentEpisodeId() && !isAuthenticated) {
                // Prevent duplicate challenge handling
                const button = document.getElementById('authButton');
                if (button.textContent.includes('SIGNING CHALLENGE')) {
                    console.log('🔄 Duplicate challenge_issued message ignored - already processing');
                    return;
                }
                handleChallenge(message.challenge);
            }
            break;
            
        case 'authentication_successful':
            if (message.episode_id === getCurrentEpisodeId() && !isAuthenticated) {
                console.log('🎯 Authentication successful message received:', message);
                handleAuthenticated(message.session_token || 'pure_p2p_authenticated');
            }
            break;
            
        case 'authentication_failed':
            if (message.episode_id === getCurrentEpisodeId()) {
                handleAuthenticationFailed(message.error);
            }
            break;
            
        case 'session_revoked':
            // Session revoked for current episode - always handle it
            if (message.episode_id === getCurrentEpisodeId()) {
                console.log('🔍 DEBUG: Session revoked for current episode');
                handleSessionRevoked();
            }
            break;
            
        case 'new_comment':
            // Defer rendering to main.js to avoid duplicate handling
            return;
    }
}

// Automatically request challenge after episode creation
export async function requestChallengeAfterEpisodeCreation() {
    // Prevent duplicate challenge requests
    if (isProcessingChallenge) {
        console.log('🔄 Challenge request already in progress - ignoring duplicate');
        return;
    }

    isProcessingChallenge = true;
    console.log('🎯 Episode created, requesting challenge...');

    const button = document.getElementById('authButton');
    button.textContent = '[ REQUESTING CHALLENGE... ]';
    button.disabled = true; // Prevent multiple clicks

    try {
        // Ensure we have the real participant public key loaded
        try {
            if (!currentWallet || !currentWallet.publicKey || currentWallet.publicKey === 'from_file') {
                const walletResponse = await resilientFetch('/wallet-participant');
                const walletData = await walletResponse.json();
                if (walletData && walletData.public_key && walletData.public_key !== 'none' && !walletData.error) {
                    if (!window.currentWallet) window.currentWallet = {};
                    currentWallet.publicKey = walletData.public_key;
                }
            }
        } catch {}

        const response = await resilientFetch('/auth/request-challenge', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                episode_id: getCurrentEpisodeId(),
                public_key: currentWallet.publicKey
            })
        });
        
        const challengeData = await response.json();
        
        if (challengeData.nonce) {
            console.log('✅ Challenge request submitted:', challengeData.nonce);
            button.textContent = '[ WAITING FOR BLOCKCHAIN... ]';
            // The challenge will be handled via WebSocket message (challenge_issued)
        } else {
            throw new Error('No challenge received from endpoint');
        }
        
    } catch (error) {
        console.error('❌ Challenge request failed:', error);
        button.textContent = '[ CHALLENGE REQUEST FAILED ]';
        button.disabled = false; // Re-enable on error
        isProcessingChallenge = false; // Reset state lock
        typewriterEffect(`CHALLENGE ERROR: ${error.message}`, button.parentElement);
    }
}

// Handle challenge received via WebSocket
export async function handleChallenge(challenge) {
    console.log('🎲 Challenge received:', challenge);
    const button = document.getElementById('authButton');
    button.textContent = '[ SIGNING CHALLENGE... ]';
    button.disabled = true; // Prevent multiple submissions
    
    try {
        // Use the actual challenge as the nonce (not a timestamp!)
        const nonce = challenge;
        
        // Get real signature from server-side signing endpoint
        const signResponse = await resilientFetch('/auth/sign-challenge', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                challenge: challenge,
                private_key: "use_participant_wallet"
            })
        });
        
        const signData = await signResponse.json();
        if (signData.error) {
            throw new Error('Signing failed: ' + signData.error);
        }
        
        const signature = signData.signature;
            
            // Submit response
            const verifyResponse = await resilientFetch('/auth/verify', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    episode_id: getCurrentEpisodeId(),
                    signature: signature,
                    nonce: nonce
                })
            });
            
            const verifyData = await verifyResponse.json();
            
            if (verifyData.status === 'submit_response_submitted') {
                button.textContent = '[ WAITING FOR AUTHENTICATION... ]';
                typewriterEffect('CHALLENGE SIGNED. WAITING FOR BLOCKCHAIN CONFIRMATION...', button.parentElement);
            } else if (verifyData.status === 'already_authenticated') {
                console.log('🔄 Authentication already completed - no duplicate transaction needed');
                handleAuthenticated(currentSessionToken || 'existing_session');
            } else if (verifyData.status === 'request_in_progress') {
                console.log('🔄 Duplicate request blocked - authentication already in progress');
                button.textContent = '[ AUTHENTICATION IN PROGRESS... ]';
                button.disabled = false; // Re-enable button for user retry
                // Don't throw error, just wait for WebSocket update
            } else {
                throw new Error('Failed to submit response: ' + verifyData.status);
            }
    } catch (error) {
        console.error('Challenge handling failed:', error);
        button.textContent = '[ ERROR - TRY AGAIN ]';
        button.disabled = false;
        isProcessingChallenge = false; // Reset state lock on error
    }
}

// Handle successful authentication
export function handleAuthenticated(sessionToken) {
    console.log('✅ Authentication successful! Session token:', sessionToken);
    console.log('🔍 DEBUG: handleAuthenticated called - about to show logout button');
    
    currentSessionToken = sessionToken;
    isAuthenticated = true;
    isProcessingChallenge = false; // Reset state lock on success
    isProcessingEpisodeCreation = false; // Reset episode creation lock on success
    
    // Update global window state for cross-module access
    window.currentSessionToken = sessionToken;
    window.isAuthenticated = true;
    window.currentEpisodeId = getCurrentEpisodeId();
    try { window.silentAuthInProgress = false; window.deferAuthPanel = false; } catch {}

    // Persist for refresh restore
    try {
        localStorage.setItem('last_episode_id', String(window.currentEpisodeId));
        localStorage.setItem('last_session_token', String(sessionToken));
        if (currentWallet && currentWallet.publicKey) {
            localStorage.setItem('participant_pubkey', currentWallet.publicKey);
        }
    } catch {}
    
    const button = document.getElementById('authButton');
    button.textContent = '[ EPISODE AUTHENTICATED ]';
    button.style.background = 'var(--success)';
    button.style.borderColor = 'var(--success)';
    button.style.color = 'var(--bg-black)';
    button.disabled = true; // Disable button to prevent multiple authentication attempts
    
    // Hide auth panel and show comment form
    const authPanel = document.getElementById('authPanel');
    if (authPanel) authPanel.style.display = 'none';

    // Show logout button
    const logoutBtn = document.getElementById('logoutButton');
    if (logoutBtn) {
        logoutBtn.style.display = 'block';
        logoutBtn.addEventListener('click', logout); // Attach listener here
        console.log('🔍 DEBUG: Logout button shown after authentication success');
    }
    
    // Show comment form with authenticated features
    showCommentForm(true);

    // Update top bar auth indicator
    try { if (window.updateTopBarAuth) window.updateTopBarAuth(true); } catch {}

    // Reveal feed panel now that membership/auth is confirmed
    try { const c = document.getElementById('commentsContainer'); if (c) c.style.display = 'block'; } catch {}

    // Visual cue: authenticated via indexer/chain
    try {
        let badge = document.getElementById('authRestoredBadge');
        if (!badge) {
            badge = document.createElement('div');
            badge.id = 'authRestoredBadge';
            badge.style.cssText = 'margin-top:8px;padding:6px 10px;border:1px solid #15e6d1;color:#15e6d1;background:rgba(21,230,209,0.06);border-radius:6px;font:12px monospace;display:inline-block;';
            const container = document.getElementById('commentForm') || document.body;
            container.parentElement?.insertBefore(badge, container);
        }
        const via = sessionToken === 'pure_p2p' || sessionToken === 'pure_p2p_authenticated' ? 'indexer/chain' : 'session';
        badge.textContent = `✅ Authenticated (${via}) — ready to comment`;
    } catch {}
    
    typewriterEffect(`LOGIN SUCCESSFUL! WELCOME TO KASPA NETWORK.`, button.parentElement);
}

// Logout function - revokes session on blockchain
export async function logout() {
    if (!currentSessionToken || !getCurrentEpisodeId()) {
        console.log('No active session to logout');
        return;
    }
    
    // Prevent duplicate logout requests
    if (isProcessingLogout) {
        console.log('🔄 Logout already in progress - ignoring duplicate');
        return;
    }
    
    isProcessingLogout = true;
    const button = document.getElementById('logoutButton');
    const originalText = button.textContent;
    button.textContent = '[ REVOKING SESSION... ]';
    button.disabled = true;
    
    try {
        // Generate signature for session token (proof of ownership)
        const signResponse = await resilientFetch('/auth/sign-challenge', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                challenge: currentSessionToken, // Sign the session token itself
                private_key: "use_participant_wallet"
            })
        });
        
        const signData = await signResponse.json();
        if (signData.error) {
            throw new Error('Failed to sign session token: ' + signData.error);
        }
        
        const response = await resilientFetch('/auth/revoke-session', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                episode_id: getCurrentEpisodeId(),
                session_token: currentSessionToken,
                signature: signData.signature
            })
        });
        
        const data = await response.json();
        
        if (data.status === 'session_revocation_submitted') {
            button.textContent = '[ WAITING FOR BLOCKCHAIN... ]';
            typewriterEffect('Session revocation submitted to blockchain...', button.parentElement);
            // WebSocket will handle the actual logout when blockchain confirms
        } else {
            throw new Error('Failed to revoke session: ' + data.status);
        }
    } catch (error) {
        console.error('Logout failed:', error);
        button.textContent = originalText;
        button.disabled = false;
        isProcessingLogout = false; // Reset state lock on error
        typewriterEffect(`LOGOUT ERROR: ${error.message}`, button.parentElement);
    }
}

// Handle authentication failure
export function handleAuthenticationFailed(error) {
    console.error('❌ Authentication failed:', error);
    
    const button = document.getElementById('authButton');
    button.textContent = '[ AUTHENTICATION FAILED ]';
    button.style.background = 'var(--error)';
    button.style.borderColor = 'var(--error)';
    button.disabled = false;
    
    typewriterEffect(`AUTHENTICATION FAILED: ${error}`, button.parentElement);
}

// Handle session revocation
export function handleSessionRevoked() {
    console.log('🚪 Session revoked');
    
    isAuthenticated = false;
    currentSessionToken = null;
    isProcessingChallenge = false; // Reset state lock
    isProcessingLogout = false; // Reset logout state lock
    isProcessingEpisodeCreation = false; // Reset episode creation lock
    
    // Hide comment form and logout button
    document.getElementById('commentForm').style.display = 'none';
    document.getElementById('logoutButton').style.display = 'none';
    
    // Reset connect button
    const button = document.getElementById('authButton');
    button.textContent = '[ OR CREATE NEW COMMENT ROOM ]';
    button.style.background = 'transparent';
    button.style.borderColor = 'var(--primary-teal)';
    button.style.color = 'var(--bright-teal)';
    button.disabled = false;
    
    typewriterEffect('SESSION REVOKED. RELOADING PAGE FOR FRESH START...', button.parentElement);
    try { if (window.updateTopBarAuth) window.updateTopBarAuth(false); } catch {}
    // Clear persisted state
    try {
        localStorage.removeItem('last_episode_id');
        localStorage.removeItem('last_session_token');
    } catch {}
    
    // Force browser restart after logout to clear all state
    setTimeout(() => {
        window.location.reload();
    }, 2000);
}

// Attempt to restore an authenticated session after page refresh
export async function tryRestoreSession() {
    try {
        try { window.deferAuthPanel = true; } catch {}
        const episodeIdStr = localStorage.getItem('last_episode_id');
        let token = localStorage.getItem('last_session_token');
        let myPub = localStorage.getItem('participant_pubkey');
        if (!episodeIdStr) return false;
        const episodeId = parseInt(episodeIdStr, 10);
        if (!episodeId) return false;

        // Always restore feed view via indexer
        setCurrentEpisodeId(episodeId);
        document.getElementById('episodeId').textContent = episodeId;
        const disp = document.getElementById('authEpisodeDisplay');
        if (disp) disp.textContent = episodeId;
        try { (await import('./commentSection.js')).loadFeedForEpisode(episodeId); } catch {}

        // Show comment form proactively; backend will enforce auth
        try {
            const authPanel = document.getElementById('authPanel');
            if (authPanel) authPanel.style.display = 'none';
            showCommentForm(true);
            if (window.updateTopBarAuth) window.updateTopBarAuth(false);
        } catch {}

        // Ensure we have a pubkey (load from backend if needed, without creating a new wallet)
        if (!myPub) {
            try {
                const r = await resilientFetch('/wallet-participant');
                const j = await r.json();
                if (j && j.public_key && j.public_key !== 'none' && !j.error) {
                    myPub = j.public_key;
                    localStorage.setItem('participant_pubkey', myPub);
                }
            } catch {}
        }

        // Always attempt backend status restore first (this may return deterministic session handle)
        try {
            const qs = myPub ? `?pubkey=${encodeURIComponent(myPub)}` : '';
            const res = await resilientFetch(`/auth/status/${episodeId}${qs}`);
            const data = await res.json();
            if (data && data.authenticated) {
                const newToken = (data.session_token && String(data.session_token)) || (myPub ? await computeDeterministicHandle(episodeId, myPub) : (token || ''));
                if (newToken) {
                    try { localStorage.setItem('last_session_token', newToken); } catch {}
                    window.currentSessionToken = newToken;
                }
                window.indexerMember = true;
                handleAuthenticated(newToken || '');
                try { window.deferAuthPanel = false; } catch {}
                return true;
            } else {
                // Start a silent re-auth in the background if we have a wallet and pubkey
                try {
                    if (window.currentWallet && (window.currentWallet.publicKey || myPub)) {
                        if (!window.currentWallet.publicKey || window.currentWallet.publicKey === 'from_file') {
                            window.currentWallet = window.currentWallet || {};
                            window.currentWallet.publicKey = myPub || window.currentWallet.publicKey;
                        }
                        console.log('🔁 Starting silent re-auth to restore session');
                        requestChallengeAfterEpisodeCreation();
                    }
                } catch {}
            }
        } catch {}

        // Pure P2P: check membership via indexer
        if (myPub) {
            const base = localStorage.getItem('indexerUrl') || 'http://127.0.0.1:8090';
            try {
                const resp = await fetch(`${base}/index/me/${episodeId}?pubkey=${myPub}`);
                if (resp.ok) {
                    const j = await resp.json();
                    if (j && j.member) {
                        // Compute deterministic handle on client as fallback
                        const h = await computeDeterministicHandle(episodeId, myPub);
                        try { localStorage.setItem('last_session_token', h); } catch {}
                        window.currentSessionToken = h;
                        window.indexerMember = true;
                        handleAuthenticated(h);
                        try { window.deferAuthPanel = false; } catch {}
                        return true;
                    }
                }
            } catch {}
        }

        // Feed restored, but not authenticated; allow panel to show now
        try { window.deferAuthPanel = false; } catch {}
        // If we have a wallet loaded, show the auth panel explicitly
        try { if (!window.isAuthenticated && (window.currentWallet || (await (async()=>false)()))) { (await import('./walletManager.js')).showAuthPanel(); } } catch {}
        return true; // feed restored, even if not authenticated
    } catch (e) {
        console.warn('Session restore failed', e);
        try { window.deferAuthPanel = false; } catch {}
        // If restore fails entirely, and wallet exists, show auth panel
        try { if (!window.isAuthenticated && window.currentWallet) { (await import('./walletManager.js')).showAuthPanel(); } } catch {}
        return false;
    }
}

async function computeDeterministicHandle(episodeId, pubkeyHex) {
    // Simple SHA-256( "KDAPP/COMMENT-IT/SESSION" || u64_be(episodeId) || pubkey_hex ) in JS
    try {
        const enc = new TextEncoder();
        const prefix = enc.encode('KDAPP/COMMENT-IT/SESSION');
        const buf = new ArrayBuffer(8);
        const view = new DataView(buf);
        view.setUint32(0, Math.floor(episodeId / 2 ** 32));
        view.setUint32(4, episodeId >>> 0);
        const idBytes = new Uint8Array(buf);
        const pubBytes = enc.encode(pubkeyHex);
        const toHash = new Uint8Array(prefix.length + idBytes.length + pubBytes.length);
        toHash.set(prefix, 0);
        toHash.set(idBytes, prefix.length);
        toHash.set(pubBytes, prefix.length + idBytes.length);
        const digest = await crypto.subtle.digest('SHA-256', toHash);
        const hex = [...new Uint8Array(digest)].map(b => b.toString(16).padStart(2, '0')).join('');
        return hex;
    } catch {
        // Fallback: stable string
        return `h_${episodeId}_${pubkeyHex.slice(0,16)}`;
    }
}

// Handle anonymous mode
export function handleAnonymousMode() {
    const isAnonymous = document.getElementById('anonMode').checked;
    
    if (isAnonymous) {
        // Generate temporary anonymous identity
        const anonId = 'ANON_' + Math.random().toString(36).substr(2, 8).toUpperCase();
        document.getElementById('walletAddress').textContent = anonId;
        document.getElementById('episodeId').textContent = 'TEMP_' + Math.floor(Math.random() * 10000);
        
        // Show comment form with anonymous features
        showCommentForm(false);
        
        // Hide authentication panel
        document.querySelector('#authPanel').style.display = 'none';
    } else {
        // Show authentication panel only if not already authenticated
        if (!isAuthenticated) {
            document.querySelector('#authPanel').style.display = 'block';
            document.getElementById('commentForm').style.display = 'none';
            
            // Reset participant info
            if (currentWallet) {
                document.getElementById('walletAddress').textContent = truncateKaspaAddress(currentWallet.kaspaAddress);
            } else {
                document.getElementById('walletAddress').textContent = 'kaspa:qrxx...v8wz';
            }
            document.getElementById('episodeId').textContent = getCurrentEpisodeId() || '--';
        }
    }
}

// Global state - DECLARE FIRST
window.availableOrganizers = [
    { name: 'local-development', url: window.location.origin, priority: 1, enabled: true },
    { name: 'project-official', url: 'https://comments1.kaspa.community', priority: 2, enabled: false },
    { name: 'community-backup', url: 'https://comments2.kaspa.community', priority: 3, enabled: false }
];

// Expose functions to the global scope for onclick attributes
window.connectWallet = connectWallet;
window.logout = logout;
window.handleAnonymousMode = handleAnonymousMode;
window.handleWebSocketMessage = handleWebSocketMessage;
window.handleAuthenticated = handleAuthenticated;
// currentEpisodeId is now managed by window.currentEpisodeId directly
window.currentSessionToken = currentSessionToken;
window.isAuthenticated = isAuthenticated;
