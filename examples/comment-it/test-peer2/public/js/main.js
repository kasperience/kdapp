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
window.availableOrganizers = [
    { name: 'test-peer2-organizer', url: 'http://localhost:8081', priority: 1, enabled: true },
    { name: 'main-organizer', url: 'http://localhost:8080', priority: 2, enabled: true },
    { name: 'project-official', url: 'https://comments1.kaspa.community', priority: 2, enabled: false },
    { name: 'community-backup', url: 'https://comments2.kaspa.community', priority: 3, enabled: false }
];

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

    // Initialize organizer peer count
    const orgPeersElement = document.getElementById('organizerPeers');
    if (orgPeersElement) {
        const enabledOrganizers = window.availableOrganizers.filter(org => org.enabled);
        orgPeersElement.textContent = enabledOrganizers.length;
    }

    // Expose handleWebSocketMessage and handleAuthenticated to global scope for WebSocket and authForm
    window.handleWebSocketMessage = handleWebSocketMessage;
    window.handleAuthenticated = handleAuthenticated;
    window.showCommentForm = showCommentForm;
    window.addNewComment = addNewComment;
});


