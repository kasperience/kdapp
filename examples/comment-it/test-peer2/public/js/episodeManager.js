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
            data.episodes.forEach(episode => {
                const roomElement = document.createElement('div');
                roomElement.className = 'active-room-item';
                roomElement.innerHTML = `
                    <span>Episode ID: ${episode.episode_id}</span>
                    <span>Creator: ${episode.creator_public_key.substring(0, 10)}...</span>
                    <button class="join-room-btn" data-episode-id="${episode.episode_id}">Join</button>
                `;
                roomsList.appendChild(roomElement);
            });
            roomsPanel.style.display = 'block';
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
    // Here you would typically set the currentEpisodeId and proceed with authentication
    window.currentEpisodeId = episodeId;
    // For now, just log it and update the UI
    document.getElementById('episodeId').textContent = episodeId;
    alert(`You have joined episode ${episodeId}. You can now submit comments.`);
    // You might want to hide the auth panel and show the comment form
    document.getElementById('authPanel').style.display = 'none';
    document.getElementById('commentForm').style.display = 'block';
    document.getElementById('authEpisodeDisplay').textContent = episodeId;
}