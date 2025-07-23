
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

function joinEpisode(episodeId) {
    console.log(`Joining episode: ${episodeId}`);
    
    // Set the current episode ID to join the existing episode
    window.currentEpisodeId = episodeId;
    
    // Use proper setter function from authForm module
    import('./authForm.js').then(module => {
        module.setCurrentEpisodeId(episodeId);
    });
    
    // Update UI to reflect joined episode
    document.getElementById('episodeId').textContent = episodeId;
    document.getElementById('authEpisodeDisplay').textContent = episodeId;
    
    // Connect WebSocket to listen for episode events
    import('./authForm.js').then(module => {
        if (!module.webSocket || module.webSocket.readyState !== WebSocket.OPEN) {
            module.connectWebSocket();
        }
    });
    
    // Check if user is already authenticated - if so, show comment form immediately
    if (window.isAuthenticated && window.currentSessionToken) {
        // Already authenticated - can participate immediately
        document.getElementById('authPanel').style.display = 'none';
        document.getElementById('commentForm').style.display = 'block';
        alert(`✅ Joined comment room ${episodeId}! You're already authenticated and can submit comments.`);
    } else {
        // Not authenticated - need to authenticate for this episode
        // Show auth panel but don't create new episode - join existing one
        document.getElementById('authPanel').style.display = 'block';
        document.getElementById('commentForm').style.display = 'none';
        
        const authButton = document.getElementById('authButton');
        authButton.textContent = '[ AUTHENTICATE FOR ROOM ]';
        
        alert(`Joined comment room ${episodeId}. Click "AUTHENTICATE FOR ROOM" to participate in authenticated comments.`);
    }
}
