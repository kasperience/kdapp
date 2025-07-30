# WebSocket Integration Guide for Kaspa-Auth

## Overview

The WebSocket client example (`websocket-client-example.html`) is a **demonstration** of how authentication could work with a cleaner, WebSocket-first architecture. Here's how it relates to your existing code:

## File Locations & Purpose

### Current HTTP-based Files
```
public/index.html                    # Existing HTTP-based web UI (WORKING)
src/api/http/organizer_peer.rs      # HTTP server (port 8080)
src/api/http/handlers/*.rs          # HTTP endpoint handlers
```

### Proposed WebSocket Files
```
examples/pure-p2p/public/index.html  # NEW WebSocket client demo
src/api/websocket/auth_handler.rs   # NEW WebSocket server implementation
src/api/websocket/mod.rs            # NEW WebSocket module
```

## Integration Approaches

### 1. **Side-by-Side Demo** (Recommended for Testing)
Keep both approaches running simultaneously:

```bash
# Terminal 1: HTTP server (existing)
cargo run --bin kaspa-auth -- http-organizer-peer --port 8080

# Terminal 2: WebSocket server (new)
cargo run --bin kaspa-auth -- websocket-peer --port 8081
```

Access:
- HTTP version: http://localhost:8080 (existing)
- WebSocket version: http://localhost:8081 (new demo)

### 2. **Unified Server** (Production Approach)
Add WebSocket endpoint to existing HTTP server:

```rust
// In src/api/http/organizer_peer.rs
.route("/ws", get(websocket_handler))     // WebSocket endpoint
.route("/ws-demo", get(serve_ws_demo))     // Serve WebSocket demo page
```

### 3. **Daemon Integration**
The daemon can support both protocols:

```rust
// HTTP client mode (current)
daemon.authenticate_http(username, "http://localhost:8080")

// WebSocket client mode (new)
daemon.authenticate_websocket(username, "ws://localhost:8081")
```

## Migration Path

### Phase 1: Parallel Development
- Fix HTTP endpoints to use proper 3-transaction flow
- Develop WebSocket version in `examples/pure-p2p/`
- Keep both running for comparison

### Phase 2: Feature Parity
- Ensure WebSocket version has all features
- Add WebSocket support to daemon
- Test both approaches thoroughly

### Phase 3: Gradual Migration
- Add WebSocket endpoint to main server
- Update clients to prefer WebSocket
- Keep HTTP for backwards compatibility

### Phase 4: WebSocket Primary
- Make WebSocket the default
- Deprecate HTTP endpoints
- Maintain HTTP for legacy support

## Quick Test Commands

```bash
# Test HTTP version (existing)
curl -X POST http://localhost:8080/auth/start \
  -H "Content-Type: application/json" \
  -d '{"public_key":"02abc..."}'

# Test WebSocket version (new)
# Use the websocket-client-example.html in browser
# Or use wscat:
wscat -c ws://localhost:8081
> {"type":"StartAuth","public_key":"02abc..."}
```

## Benefits of WebSocket Approach

1. **Real-time Updates**: No polling needed
2. **Cleaner Code**: ~600 lines vs 2000+
3. **Better UX**: Instant feedback
4. **True P2P**: Direct event streaming
5. **Lower Latency**: Persistent connection

## Decision Points

1. **Do you want to replace the HTTP version entirely?**
   - If yes: Implement WebSocket in main codebase
   - If no: Keep as separate example in `examples/pure-p2p/`

2. **Should the daemon support WebSocket?**
   - If yes: Add WebSocket client to daemon
   - If no: Keep daemon HTTP-only

3. **Timeline for migration?**
   - Immediate: Replace HTTP handlers now
   - Gradual: Run both in parallel
   - Future: Keep as example for now

## Recommended Next Steps

1. **Fix the HTTP handlers first** (critical bug fix)
   - Apply the fixed handlers from `fixed-http-handlers`
   - This makes HTTP work properly with 3 transactions

2. **Build WebSocket prototype** (innovation)
   - Create `examples/pure-p2p/` as shown in your plan
   - Demonstrate the cleaner architecture

3. **Evaluate and decide** (strategic)
   - Compare performance and complexity
   - Get user feedback
   - Make informed decision about migration