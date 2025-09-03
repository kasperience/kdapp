
// episodeManager.js

export async function fetchAndDisplayActiveEpisodes() {
    try {
        const response = await fetch('/episodes');
        if (!response.ok) {
            throw new Error(`HTTP error! status: ${response.status}`);
        }
        const data = await response.json();
        const roomsList = document.getElementById('activeRoomsList');
        const roomsPanel = document.getElementById('activeRoomsPanel');
        roomsList.innerHTML = ''; // Clear previous list

        if (data.episodes && data.episodes.length > 0) {
            // Remove duplicates by episode_id
            const uniqueEpisodes = data.episodes.reduce((acc, episode) => {
                if (!acc.find(e => e.episode_id === episode.episode_id)) {
                    acc.push(episode);
                }
                return acc;
            }, []);
            
            uniqueEpisodes.forEach(episode => {
                const roomElement = document.createElement('div');
                roomElement.className = 'active-room-item';
                roomElement.innerHTML = `
                    <span>Room Code: ${episode.room_code}</span>
                    <span>Creator: ${episode.creator_public_key.substring(0, 10)}...</span>
                    <button class="join-room-btn" data-episode-id="${episode.episode_id}">Join Room</button>
                `;
                roomsList.appendChild(roomElement);
            });
            roomsPanel.style.display = 'block';
            console.log(`📋 Displayed ${uniqueEpisodes.length} unique rooms (filtered from ${data.episodes.length} total)`);
        } else {
            roomsList.innerHTML = '<p style="color: var(--primary-teal);">No active rooms found. Be the first to create one!</p>';
            roomsPanel.style.display = 'block';
        }

        // Add event listeners to the join buttons
        document.querySelectorAll('.join-room-btn').forEach(button => {
            button.addEventListener('click', (event) => {
                const episodeId = event.target.getAttribute('data-episode-id');
                joinEpisode(episodeId);
            });
        });

    } catch (error) {
        console.error('Could not fetch active episodes:', error);
        const roomsList = document.getElementById('activeRoomsList');
        roomsList.innerHTML = '<p style="color: var(--warning);">Error fetching active rooms.</p>';
    }
}

export function joinEpisode(episodeId) {
    console.log(`Joining episode: ${episodeId}`);
    const numericEpisodeId = parseInt(episodeId, 10);

    if (isNaN(numericEpisodeId)) {
        console.error('Invalid episode ID:', episodeId);
        alert('Error: Invalid room code provided.');
        return;
    }
    
    // Set the current episode ID to join the existing episode
    window.currentEpisodeId = numericEpisodeId;
    
    // Use proper setter function from authForm module
    import('./authForm.js').then(module => {
        module.setCurrentEpisodeId(numericEpisodeId);
    });
    
    // Update UI to reflect joined episode
    document.getElementById('episodeId').textContent = numericEpisodeId;
    document.getElementById('authEpisodeDisplay').textContent = numericEpisodeId;

    // Load persistent feed from indexer
    import('./commentSection.js').then(m => m.loadFeedForEpisode(numericEpisodeId)).catch(()=>{});
    
    // Connect WebSocket to listen for episode events
    import('./authForm.js').then(module => {
        if (!module.webSocket || module.webSocket.readyState !== WebSocket.OPEN) {
            module.connectWebSocket();
        }
    });
    
    // Check if user is already authenticated - if so, show comment form immediately
    // Always show comment form; backend enforces auth. Use top bar for auth status.
    try { document.getElementById('authPanel').style.display = 'none'; } catch {}
    try { document.getElementById('commentForm').style.display = 'block'; } catch {}
    try { if (window.updateTopBarAuth) window.updateTopBarAuth(window.isAuthenticated && !!window.currentSessionToken); } catch {}

    // Start silent auth if wallet is available and not already authenticated
    if (!(window.isAuthenticated && window.currentSessionToken)) {
        try {
            if (window.currentWallet && (window.currentWallet.publicKey || window.currentWallet.kaspaAddress)) {
                console.log('🔁 Starting silent authentication for joined room');
                import('./authForm.js').then(m => m.requestChallengeAfterEpisodeCreation());
            }
        } catch {}
    }
}
