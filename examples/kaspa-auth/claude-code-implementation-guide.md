# Claude Code Implementation Guide: Fix Kaspa-Auth Authentication

## Context
The kaspa-auth authentication is failing because HTTP endpoints are trying to submit blockchain transactions at every step, but the working Web UI only submits transactions in the `/auth/verify` endpoint. This causes a mismatch: Web UI does 3 transactions, but daemon/CLI only does 2.

## Critical Fix Required: HTTP Handlers

### Step 1: Fix `/auth/start` Handler
**File**: `src/api/http/handlers/auth.rs`

**Current Problem**: Tries to submit NewEpisode transaction
**Fix**: Make it HTTP coordination only

```rust
// REPLACE the entire start_auth function with:
pub async fn start_auth(
    State(state): State<PeerState>,
    Json(req): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, StatusCode> {
    println!("🚀 Starting authentication episode (HTTP coordination)...");
    
    // Parse participant's public key
    let participant_pubkey = match hex::decode(&req.public_key) {
        Ok(bytes) => match secp256k1::PublicKey::from_slice(&bytes) {
            Ok(pk) => PubKey(pk),
            Err(_) => return Err(StatusCode::BAD_REQUEST),
        },
        Err(_) => return Err(StatusCode::BAD_REQUEST),
    };
    
    // Generate episode ID
    let episode_id: u64 = rand::thread_rng().gen();
    
    // Create participant address for display
    let participant_addr = Address::new(
        Prefix::Testnet, 
        Version::PubKey, 
        &participant_pubkey.0.x_only_public_key().0.serialize()
    );
    
    // Create in-memory episode for coordination
    let mut episode = SimpleAuth::initialize(
        vec![participant_pubkey],
        &kdapp::episode::PayloadMetadata::default()
    );
    
    // Store in coordination state (NOT blockchain yet!)
    {
        let mut episodes = state.blockchain_episodes.lock().unwrap();
        episodes.insert(episode_id, episode);
    }
    
    println!("✅ Episode {} created for HTTP coordination", episode_id);
    println!("📝 Participant should submit NewEpisode transaction themselves");
    
    Ok(Json(AuthResponse {
        episode_id,
        organizer_public_key: hex::encode(state.peer_keypair.public_key().serialize()),
        participant_kaspa_address: participant_addr.to_string(),
        transaction_id: None, // No transaction - just coordination!
        status: "episode_created_awaiting_blockchain".to_string(),
    }))
}
```

### Step 2: Fix `/auth/request-challenge` Handler
**File**: `src/api/http/handlers/challenge.rs`

**Current Problem**: Tries to submit RequestChallenge transaction
**Fix**: Return challenge immediately from memory

```rust
// REPLACE the entire request_challenge function with:
pub async fn request_challenge(
    State(state): State<PeerState>,
    Json(req): Json<ChallengeRequest>,
) -> Result<Json<ChallengeResponse>, StatusCode> {
    println!("📨 Processing challenge request (HTTP coordination)...");
    
    let episode_id: u64 = req.episode_id;
    
    // Execute challenge generation in memory
    {
        let mut episodes = state.blockchain_episodes.lock().unwrap();
        if let Some(episode) = episodes.get_mut(&episode_id) {
            // Generate challenge locally for coordination
            let challenge_cmd = AuthCommand::RequestChallenge;
            match episode.execute(
                &challenge_cmd,
                episode.owner,
                &kdapp::episode::PayloadMetadata::default()
            ) {
                Ok(_) => {
                    if let Some(challenge) = &episode.challenge {
                        println!("✅ Challenge generated: {}", challenge);
                        
                        // Return challenge immediately (no blockchain wait!)
                        return Ok(Json(ChallengeResponse {
                            episode_id,
                            nonce: challenge.clone(),
                            transaction_id: None, // No transaction!
                            status: "challenge_ready".to_string(),
                        }));
                    }
                }
                Err(e) => {
                    println!("❌ Challenge generation failed: {:?}", e);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
        }
    }
    
    Err(StatusCode::NOT_FOUND)
}
```

### Step 3: Fix `/auth/verify` Handler
**File**: `src/api/http/handlers/verify.rs`

**Current Problem**: Only submits SubmitResponse transaction
**Fix**: Submit ALL 3 transactions here

```rust
// REPLACE the verify_auth function with the one from "Fixed HTTP Handlers - Coordination Only"
// This is the ONLY place where blockchain transactions happen
// It submits all 3 transactions: NewEpisode, RequestChallenge, SubmitResponse
```

### Step 4: Update Daemon Authentication
**File**: `src/daemon/service.rs`

**Current Problem**: Uses broken mixed approach
**Fix**: Use the working endpoint pattern

Find the `run_working_authentication_flow` method and ensure it:
1. Calls `/auth/start` (no polling needed)
2. Calls `/auth/request-challenge` (gets challenge immediately)
3. Signs challenge locally
4. Calls `/auth/verify` (triggers all 3 blockchain transactions)
5. Polls `/auth/status/{id}` for confirmation

## Optional: WebSocket Implementation

If you want to implement the cleaner WebSocket approach:

### Step 1: Create WebSocket Module
```bash
mkdir -p src/api/websocket
touch src/api/websocket/mod.rs
touch src/api/websocket/auth_handler.rs
```

### Step 2: Add WebSocket Dependencies
In `Cargo.toml`, ensure you have:
```toml
tokio-tungstenite = "0.20"
futures-util = "0.3"
```

### Step 3: Copy WebSocket Implementation
Copy the code from "Pure WebSocket Authentication Handler" artifact into `src/api/websocket/auth_handler.rs`

### Step 4: Add WebSocket Command
In `src/cli/commands/mod.rs`, add:
```rust
WebSocketPeer(websocket_peer::WebSocketPeerCommand),
```

### Step 5: Create WebSocket Demo
Place the "WebSocket Client Example" HTML in:
- `examples/pure-p2p/public/index.html` (for demo)
- OR `/ws-demo` route in HTTP server

## Testing Instructions

### Test Fixed HTTP Endpoints:
```bash
# Terminal 1: Start server
cargo run --bin kaspa-auth -- http-organizer-peer --port 8080

# Terminal 2: Test with curl
# 1. Start auth
curl -X POST http://localhost:8080/auth/start \
  -H "Content-Type: application/json" \
  -d '{"public_key":"02abc..."}'

# 2. Request challenge (should return immediately)
curl -X POST http://localhost:8080/auth/request-challenge \
  -H "Content-Type: application/json" \
  -d '{"episode_id":12345,"public_key":"02abc..."}'

# 3. Submit verification (triggers all 3 blockchain transactions)
curl -X POST http://localhost:8080/auth/verify \
  -H "Content-Type: application/json" \
  -d '{"episode_id":12345,"signature":"...","nonce":"..."}'
```

### Test Daemon:
```bash
# Start daemon
cargo run --bin kaspa-auth -- daemon start --foreground

# In another terminal, authenticate
cargo run --bin kaspa-auth -- daemon send auth -u participant-peer -s http://localhost:8080
```

## Success Criteria

1. **HTTP endpoints work without blockchain transactions** (except `/auth/verify`)
2. **Daemon authentication succeeds** with all 3 transactions
3. **Web UI continues to work** as before
4. **Challenge is returned immediately** from `/auth/request-challenge`

## Common Issues & Solutions

**Issue**: "No UTXOs found"
**Solution**: Fund the participant address shown in logs

**Issue**: "Episode not found" 
**Solution**: Ensure `/auth/start` stores episode in `blockchain_episodes`, not just `episodes`

**Issue**: Challenge not returned immediately
**Solution**: Check that `/auth/request-challenge` generates challenge in memory, not blockchain

**Issue**: Only 2 transactions submitted
**Solution**: Ensure `/auth/verify` submits all 3 transactions in sequence

## Code Organization

```
Fixed Files:
├── src/api/http/handlers/auth.rs        # Fixed start_auth
├── src/api/http/handlers/challenge.rs   # Fixed request_challenge  
├── src/api/http/handlers/verify.rs      # Fixed verify_auth (3 txs)
└── src/daemon/service.rs                 # Fixed daemon auth flow

Optional WebSocket:
├── src/api/websocket/mod.rs             # New module
├── src/api/websocket/auth_handler.rs    # WebSocket implementation
└── examples/pure-p2p/public/index.html  # WebSocket demo
```

## Final Notes

- The key insight is that HTTP endpoints should ONLY coordinate, not submit transactions
- ALL blockchain transactions happen in `/auth/verify` 
- This matches how the working Web UI behaves
- WebSocket is optional but provides a cleaner architecture