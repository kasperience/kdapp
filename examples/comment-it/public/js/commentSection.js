import { getCurrentEpisodeId } from './authForm.js';
import { currentWallet } from './walletManager.js';

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

// Show comment form (always authenticated in pure kdapp)
export function showCommentForm() {
    const commentForm = document.getElementById('commentForm');
    const commentInput = document.getElementById('commentInput');
    const charCount = document.getElementById('charCount');
    
    commentInput.maxLength = 2000;
    commentInput.placeholder = "Enter your episode message... (2000 chars max)";
    charCount.textContent = '2000';
    
    commentInput.oninput = () => {
        const remaining = 2000 - commentInput.value.length;
        charCount.textContent = remaining;
        charCount.style.color = remaining < 200 ? 'var(--warning)' : 'var(--primary-teal)';
    };
    
    commentForm.style.display = 'block';
    document.getElementById('submitCommentBtn').addEventListener('click', submitComment);
}

export async function submitComment() {
    if (!currentWallet || !currentWallet.privateKey) {
        alert('No wallet available. Please create or import a wallet first.');
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
    
    button.textContent = '[ SUBMITTING TO BLOCKCHAIN... ]';

    try {
        // Construct the SubmitComment command as a JSON object
        const command = {
            SubmitComment: {
                text: commentText,
                episode_id: getCurrentEpisodeId() || 0 // Use 0 for initial episode creation if not set
            }
        };

        // Send the command over the WebSocket to the backend
        if (window.commandWebSocket && window.commandWebSocket.readyState === WebSocket.OPEN) {
            window.commandWebSocket.send(JSON.stringify(command));
            console.log('Command sent to backend via WebSocket:', command);
            button.textContent = '[ COMMAND SENT TO BACKEND ]';
            document.getElementById('commentInput').value = '';
        } else {
            throw new Error('WebSocket connection not open. Please ensure the backend is running.');
        }

    } catch (error) {
        console.error('Comment submission failed:', error);
        button.textContent = '[ ERROR - TRY AGAIN ]';
    }

    setTimeout(() => {
        button.textContent = originalText;
        button.disabled = false;
    }, 2000);
}

// Track displayed comments to prevent duplicates
const displayedComments = new Set();

// Handle new comment received from blockchain via WebSocket
export function handleNewComment(message) {
    console.log('🎯 P2P COMMENT RECEIVED - Adding to UI...', message.comment);
    
    const container = document.getElementById('commentsContainer');
    if (!container) {
        console.error('❌ Comments container not found');
        return;
    }
    
    // Create unique comment ID for deduplication
    const commentId = `${message.episode_id}_${message.comment.id}_${message.comment.timestamp}`;
    
    // Check if we've already displayed this comment
    if (displayedComments.has(commentId)) {
        console.log('🔄 Duplicate comment ignored:', commentId);
        return;
    }
    
    // Mark comment as displayed
    displayedComments.add(commentId);
    
    const comment = document.createElement('div');
    comment.className = 'comment-card comment-authenticated';
    comment.style.borderLeft = '4px solid var(--bright-cyan)';
    comment.style.background = 'rgba(20, 184, 166, 0.1)';
    comment.style.animation = 'comment-appear 0.5s ease-out';
    
    // Create timestamp display
    const timestamp = new Date(message.comment.timestamp * 1000);
    const timeString = timestamp.toLocaleTimeString();
    
    comment.innerHTML = `
        <div class="comment-header">
            <span class="comment-author">${message.comment.author}</span>
            <div class="comment-meta">
                <span>EPISODE: ${message.episode_id}</span>
                <span>TIME: ${timeString}</span>
                <span class="author-badge" style="background: linear-gradient(45deg, var(--success), var(--bright-cyan)); padding: 2px 8px; border-radius: 12px; font-size: 0.6rem; text-transform: uppercase;">P2P VERIFIED</span>
            </div>
        </div>
        <div class="comment-body">
            ${message.comment.text}
        }
        <div style="font-size: 0.7rem; color: var(--success); margin-top: 10px;">
            💬 REAL-TIME P2P COMMENT FROM BLOCKCHAIN
        </div>
    `;
    
    // Add to top of comments (newest first)
    container.insertBefore(comment, container.firstChild);
    
    // Update stats
    const commentCount = parseInt(document.getElementById('commentEpisodes').textContent.replace(/,/g, ''));
    document.getElementById('commentEpisodes').textContent = (commentCount + 1).toLocaleString();
    
    console.log('✅ P2P comment added to UI successfully!');
}
