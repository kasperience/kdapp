import { resilientFetch } from './utils.js';
import { isAuthenticated, currentEpisodeId, currentSessionToken } from './authForm.js';

export function initCommentForm() {
    const commentInput = document.getElementById('commentInput');
    const charCount = document.getElementById('charCount');
    
    if (commentInput && charCount) {
        commentInput.addEventListener('input', () => {
            const remaining = 1000 - commentInput.value.length;
            charCount.textContent = remaining;
            charCount.style.color = remaining < 100 ? 'var(--warning)' : 'var(--primary-teal)';
        });
    }
}

// Show comment form with different features for authenticated vs anonymous
export function showCommentForm(authenticated) {
    const commentForm = document.getElementById('commentForm');
    const commentInput = document.getElementById('commentInput');
    const charCount = document.getElementById('charCount');
    
    if (authenticated) {
        // Authenticated user features
        commentInput.maxLength = 2000;
        commentInput.placeholder = "Enter your authenticated episode message... (2000 chars max, edit window: 5 mins, replies enabled)";
        charCount.textContent = '2000';
        
        // Update character counter logic for authenticated users
        commentInput.oninput = () => {
            const remaining = 2000 - commentInput.value.length;
            charCount.textContent = remaining;
            charCount.style.color = remaining < 200 ? 'var(--warning)' : 'var(--primary-teal)';
        };
    } else {
        // Anonymous user features
        commentInput.maxLength = 1000;
        commentInput.placeholder = "Enter your anonymous episode message... (1000 chars max, no edits, no replies)";
        charCount.textContent = '1000';
        
        // Update character counter logic for anonymous users
        commentInput.oninput = () => {
            const remaining = 1000 - commentInput.value.length;
            charCount.textContent = remaining;
            charCount.style.color = remaining < 100 ? 'var(--warning)' : 'var(--primary-teal)';
        };
    }
    
    commentForm.style.display = 'block';
    document.getElementById('submitCommentBtn').addEventListener('click', submitComment);
}

// Real blockchain comment submission using participant's wallet via HTTP
async function submitCommentToBlockchain(commentText) {
    try {
        console.log('🚀 Submitting comment to blockchain via HTTP...');
        console.log('Episode ID:', currentEpisodeId);
        console.log('Session Token:', currentSessionToken);
        console.log('Comment:', commentText);
        
        const response = await resilientFetch('/api/comments', {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({
                episode_id: currentEpisodeId,
                text: commentText,
                session_token: currentSessionToken,
            }),
        });
        
        if (!response.ok) {
            throw new Error(`HTTP ${response.status}: ${response.statusText}`);
        }
        
        const result = await response.json();
        console.log('✅ Comment submitted successfully:', result);
        
        // Show success message with transaction details
        if (result.transaction_id) {
            console.log(`🔗 Transaction ID: ${result.transaction_id}`);
            console.log(`🔗 Explorer: https://explorer-tn10.kaspa.org/txs/${result.transaction_id}`);
        }
        
        return result;
    } catch (error) {
        console.error('❌ Comment submission error:', error);
        throw error;
    }
}

export function submitComment() {
    if (!isAuthenticated) {
        alert('Please authenticate first!');
        return;
    }
    
    const button = event.target;
    const originalText = button.textContent;
    button.disabled = true;
    
    const commentText = document.getElementById('commentInput').value.trim();
    if (!commentText) {
        alert('Please enter a comment!');
        button.disabled = false;
        return;
    }
    
    // Real blockchain comment submission
    button.textContent = '[ SUBMITTING TO BLOCKCHAIN... ]';
    
    // Use CLI command to submit comment (participant-funded)
    submitCommentToBlockchain(commentText)
        .then(() => {
            button.textContent = '[ COMMENT SUBMITTED TO BLOCKCHAIN ]';
            // Clear input
            document.getElementById('commentInput').value = ''; // Changed from commentText to commentInput
            setTimeout(() => {
                button.textContent = originalText;
                button.disabled = false;
            }, 2000);
        })
        .catch(error => {
            console.error('Comment submission failed:', error);
            button.textContent = '[ ERROR - TRY AGAIN ]';
            setTimeout(() => {
                button.textContent = originalText;
                button.disabled = false;
            }, 2000);
        });
}

export function addNewComment() {
    const container = document.getElementById('commentsContainer');
    const comment = document.createElement('div');
    
    const isAnonymous = !isAuthenticated && document.getElementById('anonMode').checked;
    const commentInput = document.getElementById('commentInput'); // Get commentInput here

    if (isAnonymous) {
        // Anonymous comment styling
        comment.className = 'comment-card comment-anonymous';
        comment.style.borderLeft = '4px solid rgba(255, 255, 255, 0.3)';
        comment.style.opacity = '0.8';
        
        const anonId = document.getElementById('walletAddress').textContent;
        const tempEpisodeId = document.getElementById('episodeId').textContent;
        
        comment.innerHTML = `
            <div class="comment-header">
                <span class="comment-author" style="color: rgba(255,255,255,0.6);">${anonId}</span>
                <div class="comment-meta">
                    <span>TEMP: ${tempEpisodeId}</span>
                    <span>ANON MODE</span>
                </div>
            </div>
            <div class="comment-body">
                ${commentInput.value}
            </div>
            <div style="font-size: 0.7rem; color: rgba(255,255,255,0.5); margin-top: 10px;">
                [ ANONYMOUS COMMENT - NOT VERIFIED ON BLOCKCHAIN ]
            </div>
        `;
    } else {
        // Authenticated comment styling
        comment.className = 'comment-card comment-authenticated';
        comment.style.borderLeft = '4px solid var(--bright-cyan)';
        comment.style.background = 'rgba(20, 184, 166, 0.1)';
        
        const walletAddress = document.getElementById('walletAddress').textContent;
        const episodeId = currentEpisodeId || Math.floor(Math.random() * 900000) + 100000;
        const blockHeight = parseInt(document.getElementById('blockHeight').textContent.replace(/,/g, ''));
        
        comment.innerHTML = `
            <div class="comment-header">
                <span class="comment-author">${walletAddress}</span>
                <div class="comment-meta">
                    <span>EPISODE: ${episodeId}</span>
                    <span>BLOCK: ${blockHeight.toLocaleString()}</span>
                    <span class="author-badge" style="background: linear-gradient(45deg, var(--primary-teal), var(--bright-cyan)); padding: 2px 8px; border-radius: 12px; font-size: 0.6rem; text-transform: uppercase;">VERIFIED</span>
                </div>
            </div>
            <div class="comment-body">
                ${commentInput.value}
            </div>
            <a href="#" class="verify-link">[ VERIFY ON KASPA EXPLORER → ]</a>
        `;
    }
    
    container.insertBefore(comment, container.firstChild);
    
    // Update stats
    const commentCount = parseInt(document.getElementById('commentEpisodes').textContent.replace(/,/g, ''));
    document.getElementById('commentEpisodes').textContent = (commentCount + 1).toLocaleString();
}
