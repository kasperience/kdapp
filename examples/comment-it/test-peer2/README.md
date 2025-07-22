# Test-Peer2 - Separate Wallet Testing

## 🎯 Purpose
This directory contains a **separate participant peer** for testing multi-user scenarios with **independent wallets**.

## 🔑 Wallet Separation
- **Main project**: Uses `.kaspa-auth/participant-peer-wallet.key`
- **Test-peer2**: Uses `test-peer2/.kaspa-auth/participant-peer-wallet.key`
- **Result**: Two completely separate wallets for realistic testing

## 🚀 Usage Instructions

### 1. Start Test-Peer2 Backend
```bash
# Windows
cd test-peer2
start-backend.bat

# Linux/Mac
cd test-peer2
chmod +x start-backend.sh
./start-backend.sh
```

### 2. Open Test-Peer2 Frontend
Open `test-peer2/public/index.html` in your browser

### 3. Configuration
- **Backend**: Runs on port **8081** with `KASPA_AUTH_WALLET_DIR=test-peer2/.kaspa-auth`
- **Frontend**: Prioritizes `localhost:8081` over `localhost:8080`
- **Wallet**: Uses separate wallet file in `test-peer2/.kaspa-auth/`

## 🧪 Testing Scenarios

### Multi-User Authentication
1. Start main backend: `cargo run --bin comment-it -- http-peer --port 8080`
2. Start test-peer2 backend: `cd test-peer2 && ./start-backend.sh`
3. Open main frontend: `public/index.html`
4. Open test-peer2 frontend: `test-peer2/public/index.html`
5. Each uses different wallet addresses!

### Expected Results
- **Main frontend**: Shows wallet address from `.kaspa-auth/participant-peer-wallet.key`
- **Test-peer2 frontend**: Shows wallet address from `test-peer2/.kaspa-auth/participant-peer-wallet.key`
- **No conflicts**: Each peer operates independently

## 🔧 Technical Details

### Environment Variable
Test-peer2 backend sets: `KASPA_AUTH_WALLET_DIR=test-peer2/.kaspa-auth`

### Wallet Configuration
```rust
// wallet.rs supports custom wallet directory
let wallet_dir = std::env::var("KASPA_AUTH_WALLET_DIR")
    .map(|dir| Path::new(&dir).to_path_buf())
    .unwrap_or_else(|_| Path::new(".kaspa-auth").to_path_buf());
```

### Frontend Priority
```javascript
window.availableOrganizers = [
    { name: 'test-peer2-organizer', url: 'http://localhost:8081', priority: 1 },
    { name: 'main-organizer', url: 'http://localhost:8080', priority: 2 },
    // ...
];
```

## ✅ Verification
After starting test-peer2:
1. Check wallet address in browser matches: `kaspatest:qplzs7v48e...kd9cvskj` (truncated)
2. Backend logs show: `Using wallet directory: test-peer2/.kaspa-auth/`
3. Full address matches test-peer2's wallet file

This setup enables realistic multi-user testing with independent wallets and authentication flows!