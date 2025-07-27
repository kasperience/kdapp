// Helper function to truncate Kaspa addresses for display
export function truncateKaspaAddress(address) {
    if (!address || address.length <= 28) return address;
    return address.substring(0, 20) + '...' + address.substring(address.length - 8);
}

// Typewriter effect for messages
export function typewriterEffect(text, container) {
    const div = document.createElement('div');
    div.style.color = 'var(--success)';
    div.style.marginTop = '15px';
    div.style.fontSize = '0.9rem';
    container.appendChild(div);
    
    let index = 0;
    const interval = setInterval(() => {
        if (index < text.length) {
            div.textContent += text[index];
            index++;
        } else {
            clearInterval(interval);
            setTimeout(() => div.remove(), 3000);
        }
    }, 50);
}

// Resilient P2P peer connection with automatic fallback
export async function resilientFetch(path, options = {}) {
    const enabledOrganizers = window.availableOrganizers.filter(org => org.enabled);
    
    if (enabledOrganizers.length === 0) {
        throw new Error('No enabled organizer peers available');
    }
    
    let lastError = null;
    
    for (const organizer of enabledOrganizers) {
        try {
            console.log(`🎯 Trying organizer '${organizer.name}' at ${organizer.url}`);
            
            const url = organizer.url + path;
            const response = await fetch(url, {
                ...options,
                timeout: 30000 // 30 second timeout
            });
            
            if (response.ok) {
                console.log(`✅ SUCCESS on organizer '${organizer.name}'`);
                updateOrganizerStatus(organizer.name, 'success');
                return response;
            } else if (response.status === 503) {
                // Wallet needs funding - special handling
                throw new Error(`WALLET_NEEDS_FUNDING: Your wallet needs funding for blockchain transactions. Visit https://faucet.kaspanet.io/ and fund address: ${window.currentWallet.kaspaAddress}`);
            } else {
                throw new Error(`HTTP ${response.status}: ${response.statusText}`);
            }
        } catch (error) {
            console.log(`❌ Failed on organizer '${organizer.name}': ${error.message}`);
            updateOrganizerStatus(organizer.name, 'failure');
            lastError = error;
            
            // Small delay before trying next organizer
            await new Promise(resolve => setTimeout(resolve, 1000));
        }
    }
    
    throw new Error(`All organizer peers failed. Last error: ${lastError.message}`);
}

// Update organizer status display
function updateOrganizerStatus(organizerName, status) {
    // Update peer count display based on successful connections
    const successfulConnections = window.availableOrganizers.filter(org => org.lastStatus === 'success').length;
    document.getElementById('peerCount').textContent = Math.max(1, successfulConnections);
    
    // Store status for the organizer
    const organizer = window.availableOrganizers.find(org => org.name === organizerName);
    if (organizer) {
        organizer.lastStatus = status;
        organizer.lastTried = Date.now();
    }
}
