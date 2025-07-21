import { resilientFetch, typewriterEffect, truncateKaspaAddress } from './utils.js';
import { showCommentForm, handleNewComment } from './commentSection.js';
import { currentWallet, showAuthPanel, showFundingInfo } from './walletManager.js'; // Added comment to force refresh

export let currentEpisodeId = null;
export let currentSessionToken = null;
export let isAuthenticated = false;
let isProcessingChallenge = false; // Prevent duplicate challenge processing
let isProcessingLogout = false; // Prevent duplicate logout processing

export let webSocket = null;

// Real API functions
export async function connectWallet() {
    if (!currentWallet) {
        alert('No wallet available. Please create or import a wallet first.');
        return;
    }
    const button = event.target;
    const originalText = button.textContent;
    button.textContent = '[ CONNECTING TO KASPA... ]';
    button.disabled = true;
    
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
        
        // Step 2: Start authentication episode
        const authResponse = await resilientFetch('/auth/start', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                public_key: walletData.public_key
            })
        });
        
        const authData = await authResponse.json();
        
        if (authData.status === 'submitted_to_blockchain' || authData.status === 'transaction_submission_failed') {
            currentEpisodeId = authData.episode_id;
            
            // Update UI
            document.getElementById('episodeId').textContent = currentEpisodeId;
            document.getElementById('authEpisodeDisplay').textContent = currentEpisodeId;
            
            if (authData.status === 'submitted_to_blockchain') {
                button.textContent = '[ WAITING FOR CHALLENGE... ]';
                typewriterEffect(`CONNECTED TO KASPA NETWORK. AUTHENTICATING...`, button.parentElement);
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
    }
}

// WebSocket connection for real-time updates
export function connectWebSocket() {
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
        // Attempt to reconnect after 3 seconds
        setTimeout(connectWebSocket, 3000);
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
            // Only ignore if we've already processed this specific episode AND we're not starting fresh
            if (currentEpisodeId === message.episode_id && isProcessingChallenge) {
                console.log('🔄 Duplicate episode_created message ignored - already processing');
                return;
            }
            console.log('🎯 Episode created, requesting challenge...');
            currentEpisodeId = message.episode_id; // Ensure episode ID is set
            // Only request challenge if we're not already authenticated
            if (!isAuthenticated) {
                requestChallengeAfterEpisodeCreation();
            }
            break;
            
        case 'challenge_issued':
            if (message.episode_id === currentEpisodeId && !isAuthenticated) {
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
            if (message.episode_id === currentEpisodeId && !isAuthenticated) {
                handleAuthenticated(message.session_token);
            }
            break;
            
        case 'authentication_failed':
            if (message.episode_id === currentEpisodeId) {
                handleAuthenticationFailed(message.error);
            }
            break;
            
        case 'session_revoked':
            // Session revoked for current episode - always handle it
            if (message.episode_id === currentEpisodeId) {
                console.log('🔍 DEBUG: Session revoked for current episode');
                handleSessionRevoked();
            }
            break;
            
        case 'new_comment':
            // Real-time P2P comment received from blockchain
            console.log('💬 NEW COMMENT received from blockchain:', message.comment);
            handleNewComment(message);
            break;
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
        const response = await resilientFetch('/auth/request-challenge', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                episode_id: currentEpisodeId,
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
                    episode_id: currentEpisodeId,
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
    
    const button = document.getElementById('authButton');
    button.textContent = '[ EPISODE AUTHENTICATED ]';
    button.style.background = 'var(--success)';
    button.style.borderColor = 'var(--success)';
    button.style.color = 'var(--bg-black)';
    button.disabled = true; // Disable button to prevent multiple authentication attempts
    
    // Show logout button
    const logoutBtn = document.getElementById('logoutButton');
    if (logoutBtn) {
        logoutBtn.style.display = 'block';
        logoutBtn.addEventListener('click', logout); // Attach listener here
        console.log('🔍 DEBUG: Logout button shown after authentication success');
    }
    
    // Show comment form with authenticated features
    // showCommentForm(true); // This function is in commentSection.js
    
    typewriterEffect(`LOGIN SUCCESSFUL! WELCOME TO KASPA NETWORK.`, button.parentElement);
}

// Logout function - revokes session on blockchain
export async function logout() {
    if (!currentSessionToken || !currentEpisodeId) {
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
                episode_id: currentEpisodeId,
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
    
    // Hide comment form and logout button
    document.getElementById('commentForm').style.display = 'none';
    document.getElementById('logoutButton').style.display = 'none';
    
    // Reset connect button
    const button = document.getElementById('authButton');
    button.textContent = '[ CREATE AUTH EPISODE ]';
    button.style.background = 'transparent';
    button.style.borderColor = 'var(--primary-teal)';
    button.style.color = 'var(--bright-teal)';
    button.disabled = false;
    
    typewriterEffect('SESSION REVOKED. RELOADING PAGE FOR FRESH START...', button.parentElement);
    
    // Force browser restart after logout to clear all state
    setTimeout(() => {
        window.location.reload();
    }, 2000);
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
            document.getElementById('episodeId').textContent = currentEpisodeId || '--';
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
window.currentEpisodeId = currentEpisodeId;
window.currentSessionToken = currentSessionToken;
window.isAuthenticated = isAuthenticated;
