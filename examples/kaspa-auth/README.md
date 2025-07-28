# 🔐 Kaspa Authentication - True Peer-to-Peer Authentication on Blockchain

A **hybrid peer-to-peer authentication system** built on the Kaspa blockchain using the kdapp framework. This combines the security of blockchain transactions with the reliability of HTTP coordination - a **practical P2P protocol** where participants control their own authentication.

## 🌟 What Makes This Special

### ✅ True Peer-to-Peer Architecture
- **No central server controls authentication**
- **Participants fund their own transactions** (like real P2P systems)
- **Blockchain is the only source of truth** (not databases or servers)
- **Episodes coordinate shared state** between equal peers

### 🔒 Real Cryptographic Security
- **Genuine secp256k1 signatures** (not mock crypto)
- **Challenge-response authentication** with unpredictable nonces
- **Blockchain verification** of all authentication events
- **Episode authorization** prevents unauthorized access

### ⚡ Live Blockchain Experience
- **Real-time WebSocket updates** from blockchain events
- **Transaction confirmations** visible on Kaspa explorer
- **Episode state synchronization** across all participants
- **Immediate feedback** on authentication status
- **Session management** with login/logout state and token voiding

## 🚀 Quick Start

### Prerequisites
- Rust toolchain (latest stable)
- Testnet TKAS tokens (get from [faucet](https://faucet.kaspanet.io/))
- Linux/WSL (for keychain integration) or Windows

### 🔐 Authentication as a Service (NEW!)

**Background daemon for persistent authentication identities:**

1. **Start the authentication daemon:**
   ```bash
   cargo run --bin kaspa-auth -- --keychain daemon start --foreground
   ```

2. **Create authentication identity:**
   ```bash
   cargo run --bin kaspa-auth -- daemon send create --username alice --password secure123
   ```

3. **Unlock identity for use:**
   ```bash
   cargo run --bin kaspa-auth -- daemon send unlock --username alice --password secure123
   ```

4. **Check daemon status:**
   ```bash
   cargo run --bin kaspa-auth -- daemon status
   ```

5. **Authenticate with service:**
   ```bash
   cargo run --bin kaspa-auth -- daemon send auth --username alice --server http://localhost:8080
   ```

### 🖥️ Web Interface (Classic Mode)

1. **Start the HTTP organizer peer:**
   ```bash
   cargo run --bin kaspa-auth -- http-peer --port 8080
   ```

2. **Open browser:** Navigate to `http://localhost:8080`

3. **Follow the authentication flow:**
   - Click "Start Authentication Flow"
   - **Fund YOUR participant address** (shown in console)
   - Complete challenge-response authentication
   - Watch real-time blockchain confirmations!
   - **After success**: Button changes to "Logout & Void Session"
   - **Click logout** to void session token and start fresh

### 💻 CLI Interface (Hybrid P2P)

```bash
# Start hybrid authentication (kdapp + HTTP coordination)
cargo run --bin kaspa-auth -- authenticate --peer http://localhost:8080

# Or use pure kdapp mode (experimental)
cargo run --bin kaspa-auth -- authenticate --pure-kdapp

# Fund the displayed address at https://faucet.kaspanet.io/
# Authentication uses blockchain transactions + HTTP coordination
```

### 🗂️ OS Keychain Integration

```bash
# Use OS keychain (GNOME Keyring on Linux, Credential Manager on Windows)
cargo run --bin kaspa-auth -- --keychain wallet-status

# Development mode (insecure file-based storage)
cargo run --bin kaspa-auth -- --dev-mode wallet-status

# Check wallet information stored in keychain
cargo run --bin kaspa-auth -- --keychain wallet-status --command organizer-peer
```

## 🏗️ Architecture Deep Dive

### 🎯 The P2P Philosophy

**Traditional (Broken):**
```
User → Server → Database → Server → User
      (Server controls everything)
```

**Kaspa Auth (P2P):**
```
Participant ↔ Blockchain ↔ Organizer Peer
    (Blockchain is source of truth)
```

### 🔄 Authentication Flow

1. **Episode Creation**: Participant creates authentication episode on blockchain
2. **Challenge Request**: Participant requests challenge from organizer
3. **Challenge Response**: Organizer generates cryptographic challenge
4. **Signature Verification**: Participant signs challenge and submits proof
5. **Blockchain Confirmation**: All events recorded on Kaspa blockchain
6. **Session Token**: Secure session established after verification

### 📊 Component Breakdown

```
kaspa-auth/
├── 🧠 Core Authentication Logic
│   ├── SimpleAuth Episode       # Authentication state machine
│   ├── Challenge Generation     # Cryptographic nonce creation
│   └── Signature Verification   # secp256k1 verification
├── 🔐 Authentication as a Service
│   ├── kaspa-auth-daemon       # Background service
│   ├── Unix Socket IPC         # Client-daemon communication
│   ├── Identity Management     # Secure in-memory storage
│   └── Session Tracking        # Active authentication sessions
├── 🗂️ OS Keychain Integration
│   ├── GNOME Keyring (Linux)   # Secure keychain storage
│   ├── Windows Credential Manager # Windows secure storage
│   ├── Cross-platform Support  # keyring crate v3.0.0
│   └── PIN Protection          # Authentication identity access
├── 🌐 HTTP Organizer Peer
│   ├── Web Dashboard           # Browser interface
│   ├── WebSocket Updates       # Real-time notifications
│   └── Transaction Coordination # Blockchain submission
├── 💻 CLI Participant
│   ├── Wallet Management       # Persistent key storage
│   ├── Transaction Building    # Kaspa transaction creation
│   └── Episode Interaction     # P2P communication
└── ⚡ Blockchain Integration
    ├── kdapp Engine           # Episode execution
    ├── Kaspa Node Connection  # testnet-10 integration
    └── Real-time Synchronization # State updates
```

## 🛠️ API Reference

### HTTP Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/` | Web dashboard and server info |
| `POST` | `/auth/start` | Create new authentication episode |
| `POST` | `/auth/request-challenge` | Request challenge from organizer |
| `POST` | `/auth/verify` | Submit authentication response |
| `GET` | `/auth/status/{id}` | Get episode status |
| `GET` | `/ws` | WebSocket connection |

### WebSocket Events

| Event | Description |
|-------|-------------|
| `episode_created` | New authentication episode created |
| `challenge_issued` | Challenge generated by organizer |
| `authentication_successful` | Authentication completed |
| `authentication_failed` | Authentication failed |

### Daemon IPC Protocol

**Connection**: Unix socket at `/tmp/kaspa-auth.sock`

| Request | Response | Description |
|---------|----------|-------------|
| `Ping` | `Pong{version, uptime, identities}` | Check daemon health |
| `Status` | `Status{unlocked, identities, sessions}` | Get current status |
| `CreateIdentity{username, password}` | `Success{message}` | Create new auth identity |
| `Unlock{username, password}` | `Success{message}` | Unlock identity into memory |
| `Lock` | `Success{message}` | Lock all identities |
| `SignChallenge{username, challenge}` | `Signature{signature, pubkey}` | Sign authentication challenge |
| `Authenticate{username, server_url}` | `AuthResult{success, episode_id, token}` | Full authentication flow |
| `RevokeSession{episode_id, token, username}` | `Success{message}` | Revoke active session |

### CLI Commands

```bash
# Daemon management
cargo run -- daemon start --foreground                    # Start daemon service
cargo run -- daemon stop                                  # Stop daemon
cargo run -- daemon status                                # Check status

# Identity management  
cargo run -- daemon send create --username alice --password secure123
cargo run -- daemon send unlock --username alice --password secure123
cargo run -- daemon send lock

# Authentication operations
cargo run -- daemon send ping                             # Health check
cargo run -- daemon send auth --username alice --server http://localhost:8080
cargo run -- daemon send sign --username alice --challenge auth_12345
cargo run -- daemon send sessions                         # List active sessions

# Keychain operations
cargo run -- --keychain wallet-status                     # Check keychain wallets
cargo run -- --dev-mode wallet-status                     # Check file-based wallets
```

## 💰 Funding & Economics

### Who Pays What?

- **Participants**: Fund their own authentication transactions (~0.001 TKAS per transaction)
- **Organizer**: Funds coordination and episode management (~0.001 TKAS per episode)
- **Network**: Kaspa testnet-10 (free testnet tokens)

### Transaction Types

1. **NewEpisode**: Creates authentication episode (participant pays)
2. **RequestChallenge**: Requests challenge from organizer (participant pays)
3. **SubmitResponse**: Submits authentication proof (participant pays)

## 🧪 Testing & Development

### Full Integration Test

```bash
# Test complete authentication flow
cargo run --bin kaspa-auth -- test-api-flow --server http://localhost:8080
```

### API Endpoint Testing

```bash
# Test all API endpoints
cargo run --bin kaspa-auth -- test-api
```

### Debug Commands

```bash
# Check wallet information
curl http://localhost:8080/wallet/debug

# Check funding status
curl http://localhost:8080/funding-info

# Monitor episode status
curl http://localhost:8080/auth/status/{episode_id}
```

## 🔧 Configuration

### Wallet Files (Auto-created)

- `.kaspa-auth/organizer-peer-wallet.key` - Organizer coordination wallet
- `.kaspa-auth/participant-peer-wallet.key` - Participant authentication wallet

### Network Settings

- **Network**: Kaspa testnet-10
- **Transaction Prefix**: `0x41555448` (AUTH)
- **Episode Pattern**: Authentication episodes
- **Faucet**: https://faucet.kaspanet.io/

## 🚨 Security Features

### 🛡️ Cryptographic Security

- **Real secp256k1 signatures** (no mock crypto)
- **Unpredictable challenge generation** (secure randomness)
- **Blockchain verification** of all transactions
- **Episode authorization** prevents unauthorized commands

### 🔐 P2P Security Model

- **No central authority** controls authentication
- **Participants own their keys** (non-custodial)
- **Blockchain immutability** prevents tampering
- **Episode isolation** between authentication sessions

## 🎯 Use Cases

### 🏢 Enterprise Authentication
- **Decentralized SSO** without central identity providers
- **Audit trails** on immutable blockchain
- **Multi-party authentication** for sensitive operations

### 🎮 Gaming & Social
- **Player authentication** in P2P games
- **Tournament participation** verification
- **Social platform** identity verification

### 💼 Financial Services
- **Customer authentication** for DeFi protocols
- **Multi-signature** transaction authorization
- **Compliance audit** trails

## 🌍 Deployment

### 🏠 Local Development

```bash
# Start organizer peer
cargo run --bin kaspa-auth -- http-peer --port 8080

# Start participant
cargo run --bin kaspa-auth -- authenticate
```

### 🚀 Production Deployment

```bash
# Build release version
cargo build --release

# Run with production settings
./target/release/kaspa-auth http-peer --port 8080
```

## 🤝 Contributing

We welcome contributions to make P2P authentication even better!

### 🔄 Development Flow

1. Fork the repository
2. Create feature branch (`git checkout -b feature/amazing-feature`)
3. Add tests for new functionality
4. Submit pull request

### 📝 Code Style

- Follow Rust best practices
- Add comprehensive tests
- Document public APIs
- Maintain P2P architecture principles

## 📚 Learn More

### 🎓 Educational Resources

- [kdapp Framework Documentation](https://github.com/michaelsutton/kdapp)
- [Kaspa Protocol Overview](https://kaspa.org/)
- [P2P Authentication Patterns](https://docs.kaspa.org/)

### 🛠️ Technical Deep Dives

- Episode-based state management
- Cryptographic challenge-response protocols
- Blockchain transaction verification
- WebSocket real-time synchronization

## 🏆 Achievements

- ✅ **True P2P Architecture**: No central authority
- ✅ **Real Cryptographic Security**: Genuine secp256k1 signatures  
- ✅ **Blockchain Integration**: All events on Kaspa blockchain
- ✅ **Live User Experience**: Real-time WebSocket updates
- ✅ **Authentication as a Service**: Background daemon with Unix socket IPC
- ✅ **OS Keychain Integration**: Secure storage with GNOME Keyring & Windows Credential Manager
- ✅ **Session Management**: Active session tracking with daemon persistence
- ✅ **Cross-platform Support**: Linux, Windows, and WSL compatibility
- ✅ **Production Ready**: Comprehensive error handling
- ✅ **Developer Friendly**: Full API documentation

## 🙏 Acknowledgments

- **[kdapp Framework](https://github.com/michaelsutton/kdapp)** - The foundation for P2P episodes
- **[Kaspa Blockchain](https://kaspa.org/)** - Fast, secure, and scalable blockchain
- **[Rust Community](https://rust-lang.org/)** - Amazing language and ecosystem
- **[secp256k1](https://github.com/rust-bitcoin/rust-secp256k1)** - Cryptographic security

---

**Built with ❤️ for the decentralized future**

*This is not just authentication - it's a paradigm shift towards true peer-to-peer systems.*