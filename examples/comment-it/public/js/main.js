import { createMatrixRain, initBlockHeightUpdater, initKonamiCode } from './matrixUI.js';
import { checkExistingWallet, showCreateWallet, showImportWallet, generateNewWallet, copyPrivateKey, validateAndImportWallet, proceedWithWallet, changeWallet } from './walletManager.js';
import { connectWallet, logout, handleAnonymousMode, handleWebSocketMessage, handleAuthenticated } from './authForm.js';
import { initCommentForm, submitComment, addNewComment, showCommentForm } from './commentSection.js';
import { fetchAndDisplayActiveEpisodes } from './episodeManager.js';

// Global state (moved from index.html script)
window.currentEpisodeId = null;
window.currentSessionToken = null;
window.isAuthenticated = false;
window.currentWallet = null;

// --- CONFIGURABLE WEBSOCKET PORT FOR TESTING --- 
// For peer1, use 8080. For peer2, use 8081.
const wsPort = 8080; // <<< CHANGE THIS FOR DIFFERENT PEERS
// -----------------------------------------------

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
    initBlockHeightUpdater();
    initKonamiCode();
    initCommentForm();
    checkExistingWallet();
    fetchAndDisplayActiveEpisodes();

    // Attach event listeners for wallet management
    document.getElementById('createWalletBtn').addEventListener('click', showCreateWallet);
    document.getElementById('importWalletBtn').addEventListener('click', showImportWallet);

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

    // Establish WebSocket connection to the backend
    const wsUrl = `ws://localhost:${wsPort}`; 
    window.commandWebSocket = new WebSocket(wsUrl);

    window.commandWebSocket.onopen = () => {
        console.log('✅ Command WebSocket connected to backend');
    };

    window.commandWebSocket.onmessage = (event) => {
        console.log('📨 Command WebSocket message from backend:', event.data);
        try {
            const message = JSON.parse(event.data);
            // Dispatch to appropriate handler
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
            }
        } catch (e) {
            console.error("Error parsing WebSocket message:", e);
        }
    };

    window.commandWebSocket.onerror = (error) => {
        console.error('❌ Command WebSocket error:', error);
    };

    window.commandWebSocket.onclose = () => {
        console.log('❌ Command WebSocket disconnected from backend');
        // Attempt to reconnect after a delay
        setTimeout(() => {
            console.log('Attempting to reconnect Command WebSocket...');
            window.commandWebSocket = new WebSocket(wsUrl);
        }, 3000);
    };
});