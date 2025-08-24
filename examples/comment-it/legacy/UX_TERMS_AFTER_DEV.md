
## 🎭 UX TERMINOLOGY vs ARCHITECTURAL REALITY

### ⚠️ CRITICAL: Frontend UX Language ≠ Backend Architecture

**Frontend displays user-friendly language**:
- "LOGIN WITH KASPA" (not "CREATE AUTH EPISODE")
- "SESSION ID" (not "AUTH EPISODE")  
- "LOGOUT" (not "REVOKE SESSION")
- "CONNECTING TO KASPA..." (not "CREATING AUTH EPISODE...")
- "LOGIN SUCCESSFUL!" (not "AUTHENTICATION COMPLETE!")

**Backend maintains P2P kdapp architecture**:
- Episodes (not sessions)
- Peer coordination (not client-server)
- Blockchain state (not server state)
- P2P transactions (not API calls)

### 🚨 DO NOT "ALIGN" BACKEND WITH UX LANGUAGE!

**Why UX language was simplified**:
- Users understand "Login with Google/Facebook/GitHub" patterns
- "LOGIN WITH KASPA" follows familiar conventions
- Removes blockchain complexity from user interface
- Improves adoption and accessibility

**Why backend must stay kdapp-native**:
- Episodes are the fundamental kdapp abstraction
- P2P architecture requires episode thinking
- Client-server patterns break kdapp design
- Blockchain state management needs episode lifecycle

### 📋 Translation Guide: UX ↔ Architecture

| **UX Display** | **Backend Reality** | **Reason** |
|---|---|---|
| "Login with Kaspa" | Create auth episode | Familiar login pattern |
| "Session ID: 12345" | Episode ID: 12345 | Session = user concept |
| "Logout" | Revoke session command | Simple user action |
| "Connected" | Episode initialized | Network connection metaphor |