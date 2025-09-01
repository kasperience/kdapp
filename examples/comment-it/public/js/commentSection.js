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
    commentInput.placeholder = "Enter your comment… (2000 chars max)";
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
        // Always use HTTP simple API; backend enforces auth and signs with participant wallet
        const episodeId = getCurrentEpisodeId() || 0;
        const sessionToken = window.currentSessionToken || '';
        const resp = await fetch('/api/comments/simple', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ episode_id: episodeId, text: commentText, session_token: sessionToken })
        });
        if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
        const data = await resp.json();
        console.log('✅ Comment submitted via HTTP:', data);
        button.textContent = '[ SUBMITTED ]';
        document.getElementById('commentInput').value = '';
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
    
    // Create unique comment ID for deduplication (align with indexer keying)
    // Use episode_id + comment.id only so WS and indexer snapshots match
    const commentId = `${message.episode_id}_${message.comment.id}`;
    
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
    // Backend sends UNIX milliseconds as a string; coerce to number safely
    let tsMs = 0;
    try {
        // message.comment.timestamp can be a string (e.g. "1756366681633") due to server-side stringified u64
        tsMs = Number(message.comment.timestamp);
        if (!Number.isFinite(tsMs)) tsMs = 0;
    } catch {}
    const timestamp = tsMs > 0 ? new Date(tsMs) : new Date();
    const timeString = timestamp.toLocaleString();
    
    comment.innerHTML = `
        <div class="comment-header">
            <span class="comment-author">${message.comment.author}</span>
            <div class="comment-meta">
                <span>EPISODE: ${message.episode_id}</span>
                <span>TIME: ${timeString}</span>
                <span class="author-badge" style="background: linear-gradient(45deg, var(--success), var(--bright-cyan)); color:#fff; padding: 2px 8px; border-radius: 12px; font-size: 0.7rem; font-weight: 700; text-transform: uppercase; text-shadow: 0 1px 1px rgba(0,0,0,0.35);">P2P VERIFIED</span>
            </div>
        </div>
        <div class="comment-body">
            ${message.comment.text}
        </div>
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

// ===== Indexer integration for persistent feed =====
const INDEXER_DEFAULT = 'http://127.0.0.1:8090';
function indexerBase() {
    try { return localStorage.getItem('indexerUrl') || INDEXER_DEFAULT; } catch { return INDEXER_DEFAULT; }
}

function lastSeenKey(episodeId) { return `last_seen_ts:${episodeId}`; }
function getLastSeenTs(episodeId) {
    try { return parseInt(localStorage.getItem(lastSeenKey(episodeId)) || '0', 10) || 0; } catch { return 0; }
}
function setLastSeenTs(episodeId, ts) {
    try { localStorage.setItem(lastSeenKey(episodeId), String(ts)); } catch {}
}

export async function loadFeedForEpisode(episodeId) {
    if (!episodeId) return;
    const container = document.getElementById('commentsContainer');
    if (!container) return;
    // Reset current view and deduper so we can re-render snapshot
    container.innerHTML = '';
    try { displayedComments.clear(); } catch {}
    // Ensure feed is visible when loading from indexer
    try { container.style.display = 'block'; } catch {}

    try {
        // Snapshot
        const snapRes = await fetch(`${indexerBase()}/index/episode/${episodeId}`);
        if (snapRes.ok) {
            const snap = await snapRes.json();
            if (snap && snap.recent_comments) {
                const sorted = [...snap.recent_comments].sort((a,b)=>a.timestamp-b.timestamp);
                for (const c of sorted) renderIndexerComment(c);
                const maxTs = sorted.length ? sorted[sorted.length-1].timestamp : 0;
                if (maxTs) setLastSeenTs(episodeId, maxTs);
            }
        }
        await pollNewComments(episodeId);
        startPolling(episodeId);
    } catch (e) { console.warn('indexer feed load failed', e); }
}

async function pollNewComments(episodeId) {
    const after = getLastSeenTs(episodeId) || 0;
    const res = await fetch(`${indexerBase()}/index/comments/${episodeId}?after_ts=${after}&limit=200`);
    if (!res.ok) return;
    const data = await res.json();
    if (!data || !data.comments) return;
    let maxTs = after;
    for (const c of data.comments) { renderIndexerComment(c); if (c.timestamp > maxTs) maxTs = c.timestamp; }
    if (maxTs > after) setLastSeenTs(episodeId, maxTs);
}

let pollTimer = null;
function startPolling(episodeId) {
    if (pollTimer) clearInterval(pollTimer);
    pollTimer = setInterval(() => { pollNewComments(episodeId).catch(()=>{}); }, 6000);
}

function renderIndexerComment(row) {
    const container = document.getElementById('commentsContainer');
    if (!container) return;
    const key = `${row.episode_id}_${row.comment_id}`;
    if (displayedComments.has(key)) return;
    displayedComments.add(key);
    const div = document.createElement('div');
    div.className = 'comment-card comment-authenticated';
    div.style.borderLeft = '4px solid var(--bright-cyan)';
    div.style.background = 'rgba(20, 184, 166, 0.06)';
    const timeString = new Date(row.timestamp).toLocaleString();
    div.innerHTML = `
        <div class="comment-header">
            <span class="comment-author">${row.author}</span>
            <div class="comment-meta">
                <span>EPISODE: ${row.episode_id}</span>
                <span>TIME: ${timeString}</span>
            </div>
        </div>
        <div class="comment-body">${escapeHtml(row.text)}</div>
    `;
    container.insertBefore(div, container.firstChild);
}

function escapeHtml(str) {
    return String(str)
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/\"/g, '&quot;')
        .replace(/'/g, '&#039;');
}
// Backward-compatible alias expected by main.js
export { handleNewComment as addNewComment };
