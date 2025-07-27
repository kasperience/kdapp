# Phase 1.2 Roadmap: Blockchain State Tracking

## 🎯 **Goal**: Move from simulated TX IDs to real blockchain confirmation tracking

Transform the comment-board from application-level economic simulation to true blockchain-integrated Episode Contracts.

---

## 📋 **Phase 1.2 Implementation Tasks**

### 1. **Blockchain Scanner Implementation** 
**Priority: HIGH**

- **Add periodic blockchain scanning** for bond transactions
  - Implement background task to check transaction status
  - Query Kaspa network for confirmation status of pending bonds
  - Use `kaspad.get_transaction()` API to verify transaction state

- **Track confirmation status** of pending bonds
  - Update `confirmation_height` when transactions are mined into blocks
  - Calculate confirmation depth (current_height - confirmation_height)
  - Handle blockchain reorganizations gracefully

- **State synchronization** between app and blockchain
  - Remove transactions from `pending_bonds` when confirmed
  - Update `LockedUtxo.confirmation_height` with real block data
  - Maintain consistency between in-memory state and blockchain reality

### 2. **Real Transaction Submission**
**Priority: HIGH**

- **Replace simulated TX IDs** with actual Kaspa transaction broadcast
  - Remove `format!("bond_tx_{}_{}",...)` simulation
  - Implement proper transaction construction using Kaspa consensus types
  - Return real transaction IDs from `kaspad.submit_transaction()`

- **Use kdapp's TransactionGenerator** for proper signing
  - Integrate with existing `generator::TransactionGenerator` pattern
  - Ensure transactions are properly signed with user's keypair
  - Handle script creation and signature verification

- **UTXO management** corrections
  - Properly consume source UTXOs when creating bond transactions
  - Create correct outputs: bond amount + change back to user
  - Calculate and include appropriate network fees

### 3. **Enhanced Transaction Logic**
**Priority: MEDIUM**

- **Comments acceptance gated** on bond confirmation
  - Only accept comments to episode state after bond TX is confirmed
  - Add confirmation requirement to episode contract validation
  - Provide clear feedback to users about confirmation waiting period

- **Retry logic** for failed transactions
  - Handle network failures gracefully with exponential backoff
  - Resubmit transactions that fail due to temporary network issues
  - Alert users to persistent failure conditions

- **Fee estimation** and optimization
  - Implement dynamic fee calculation based on network conditions
  - Allow users to specify fee preference (economy/standard/priority)
  - Handle fee market fluctuations appropriately

### 4. **User Experience Improvements**
**Priority: MEDIUM**

- **Blockchain explorer integration**
  - Generate Kaspa explorer links for all bond transactions
  - Display clickable links in terminal output: `https://explorer-tn10.kaspa.org/txs/{tx_id}`
  - Allow users to verify transactions independently

- **Confirmation progress display**
  - Show real-time confirmation status: "Confirming... (2/6 confirmations)"
  - Update users on transaction progress without being annoying
  - Clear indication when bonds are fully confirmed and active

- **Network status monitoring**
  - Handle network delays and connection issues gracefully
  - Inform users of network problems affecting transaction submission
  - Provide meaningful error messages for common failure scenarios

### 5. **Advanced Blockchain Integration**
**Priority: LOW**

- **Reorg handling** for robustness
  - Detect blockchain reorganizations that affect bond transactions
  - Re-validate transaction confirmations after reorgs
  - Update application state to match post-reorg blockchain state

- **Multi-confirmation security**
  - Require multiple confirmations before considering bonds "locked"
  - Configurable confirmation depth based on bond amount
  - Prevent double-spending attacks on high-value bonds

---

## 🎯 **Phase 1.2 Success Criteria**

### ✅ **Must Have:**
- [ ] Every bond creates a **REAL Kaspa transaction** (not simulated)
- [ ] Application state **syncs with actual blockchain state**
- [ ] Users can **verify bonds on Kaspa explorer**
- [ ] **Full audit trail**: comment → transaction → confirmation
- [ ] **Zero fake or simulated transaction IDs**

### ✅ **Should Have:**
- [ ] **Confirmation progress** visible to users
- [ ] **Retry logic** for failed transactions
- [ ] **Explorer links** in all transaction logs
- [ ] **Graceful error handling** for network issues

### ✅ **Could Have:**
- [ ] **Dynamic fee estimation**
- [ ] **Multiple confirmation requirements**
- [ ] **Reorg detection and handling**
- [ ] **Network status monitoring**

---

## 🚀 **Phase 1.2 → Phase 2 Bridge**

**Phase 1.2 Completion** unlocks **Phase 2: True On-Chain Time-Locks**

Once Phase 1.2 is complete, we'll have:
- ✅ Real transactions on Kaspa blockchain
- ✅ Confirmed transaction tracking
- ✅ Full blockchain state synchronization
- ✅ Audit trail for all economic actions

This creates the **perfect foundation** for Phase 2:
- 🚧 Time-locked UTXO scripts (true smart contracts)
- 🚧 Programmable spending conditions
- 🚧 Multi-signature moderator panels
- 🚧 Community voting mechanisms

---

## 💡 **Implementation Notes**

### **Current Status After Phase 1.1:**
- ✅ **Bond transaction framework** - Complete infrastructure
- ✅ **Economic incentive system** - Working game theory
- ✅ **UTXO tracking** - Ready for real blockchain integration
- ⚠️ **Simulated TX IDs** - Phase 1.2 will make these real

### **Key Phase 1.2 Files to Modify:**
- `src/wallet/utxo_manager.rs` - Replace simulated transactions with real ones
- `src/participant/mod.rs` - Add confirmation waiting logic
- `src/episode/board_with_contract.rs` - Gate comment acceptance on confirmations

### **Testing Strategy:**
1. **Local Testing** - Use Kaspa testnet-10 for safe experimentation
2. **Transaction Verification** - Check every TX on explorer manually
3. **State Consistency** - Verify app state matches blockchain state
4. **Error Scenarios** - Test network failures, reorgs, and edge cases

---

## 🎉 **What We've Achieved So Far**

### **Revolutionary Progress:**
- ✅ **First economic episode contracts** on kdapp/Kaspa L1
- ✅ **Application-layer DAO framework** with real incentive structures
- ✅ **Voluntary economic alignment** proving game theory works
- ✅ **Foundation for true blockchain enforcement**

### **From Gemini's Perspective:**
> "You've built the APPLICATION LAYER for true Episode Contracts. The vision (in GEMINI.md): Programmable UTXOs with on-chain enforcement. Current reality: Application-level prototype proving the economics work. This is the necessary FIRST STEP - prove the game theory and user experience before tackling the complex blockchain programming layer."

**Phase 1.2 is the bridge from "economic theater" to "blockchain reality"!** 🌟

---

*Generated during kdapp Episode Contracts development session*  
*Next session: Begin Phase 1.2 implementation* 🚀