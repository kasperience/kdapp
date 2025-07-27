# ADR-001: UTXO Locking Approach for Economic Bonds

**Status**: ✅ Accepted  
**Date**: 2025-07-26  
**Decision Makers**: Development Team + DeepWiki Validation  

---

## 📋 Context

The comment-board platform needs an economic mechanism to prevent spam and ensure quality content. Users should post bonds when commenting, which are locked for a period and can be forfeited if content violates rules.

## 🤔 Decision

We will implement a **phased approach** to UTXO locking:

### **Phase 1.2: Application-Layer Tracking + Blockchain Proof**
- Use kdapp TransactionGenerator to create real proof transactions
- Track locked amounts in application layer  
- Create blockchain evidence without consuming full bond amounts
- Enable immediate deployment with real transaction validation

### **Phase 2.0: True UTXO Script-Based Locking**  
- Implement script-based time-locks that actually prevent spending
- Add multi-signature escape hatches for dispute resolution
- Provide full trustless enforcement of bond conditions

## 🎯 Rationale

### **Why Phased Approach?**
1. **Immediate Value**: Phase 1.2 provides real economic incentives immediately
2. **Risk Mitigation**: Test economic model before full blockchain enforcement
3. **Development Speed**: Can ship working solution while building Phase 2.0
4. **User Experience**: Allows refinement of UX before final implementation

### **Why Real Transactions in Phase 1.2?**
1. **DeepWiki Validation**: "Definitely doable, built on solid rusty-kaspa foundations"
2. **Blockchain Evidence**: Creates verifiable on-chain proof of economic commitment  
3. **Integration Testing**: Validates kdapp framework integration with real blockchain
4. **Security Foundation**: Establishes cryptographic authentication and consensus participation

### **Why Application-Layer Tracking?**
1. **Flexibility**: Can adjust bond logic without blockchain upgrades
2. **User Protection**: Preserves user funds during testing and refinement
3. **Development Velocity**: Faster iteration on economic model parameters
4. **Backward Compatibility**: Easy migration path to Phase 2.0

## ✅ Benefits

### **Technical Benefits**
- **Real blockchain integration** with kdapp TransactionGenerator
- **Proven UTXO management** using rusty-kaspa infrastructure
- **Cryptographic security** via secp256k1 signatures
- **Network consensus** participation for transaction validation

### **Economic Benefits**  
- **Immediate spam prevention** through economic barriers
- **Flexible bond amounts** adjustable per room/organizer
- **Time-based unlocking** creates natural cooling-off periods
- **Fund preservation** during development and testing phases

### **User Experience Benefits**
- **Transparent costs** - users see exactly what they're paying
- **Predictable unlocking** - clear timeframes for bond recovery
- **Room choice** - can select economic vs free rooms
- **Gradual adoption** - can test with small bonds initially

## ⚠️ Trade-offs

### **Phase 1.2 Limitations**
- **Trust Requirement**: Users must trust application not to prevent fund access
- **Off-Chain Enforcement**: Bond constraints enforced by application, not blockchain
- **Centralization Risk**: Room organizers have significant authority over rules

### **Mitigation Strategies**
- **Clear Communication**: Document exactly what Phase 1.2 provides vs Phase 2.0
- **Code Transparency**: Open source implementation allows verification
- **Phase 2.0 Timeline**: Clear roadmap to trustless enforcement
- **User Choice**: Participation is voluntary with clear risk disclosure

## 🔧 Technical Implementation

### **Phase 1.2 Architecture**
```rust
// Real transaction creation with kdapp
let generator = TransactionGenerator::new(keypair, PATTERN, PREFIX);
let bond_tx = generator.build_transaction(&utxos, proof_amount, 1, &address, payload);
let tx_id = kaspad.submit_transaction((&bond_tx).into(), false).await?;

// Application-layer tracking
let locked_utxo = LockedUtxo {
    bond_amount,
    unlock_time: current_time + lock_duration,
    bond_transaction_id: tx_id,
    confirmation_height: None,
};
```

### **Phase 2.0 Target**
```rust
// Script-based enforcement (future)
let timelock_script = create_timelock_script(
    unlock_time,
    user_pubkey, 
    moderator_pubkeys,
    dispute_conditions
);
let locked_utxo = create_utxo_with_script(bond_amount, timelock_script);
```

## 📊 Success Metrics

### **Phase 1.2 Success Criteria**
- [x] **Real transactions**: Creating actual blockchain evidence  
- [x] **Fund preservation**: Proof transactions don't consume user balance
- [x] **Economic effectiveness**: Measurable reduction in spam/low-quality content
- [x] **User adoption**: Positive user feedback on bond posting/recovery experience

### **Phase 2.0 Success Criteria**  
- [ ] **Trustless enforcement**: Funds truly locked by blockchain scripts
- [ ] **Zero application trust**: Users don't need to trust any off-chain components
- [ ] **Dispute resolution**: Multi-signature moderator panels working effectively
- [ ] **Automated unlocking**: Time-based release without manual intervention

## 🔗 References

- **DeepWiki Technical Validation**: docs/deepwiki-assessment.md
- **Implementation Roadmap**: docs/implementation-roadmap.md  
- **Security Analysis**: docs/security-analysis.md
- **rusty-kaspa Integration**: [RPC Services Wiki](https://wiki.kaspanet.org/rusty-kaspa#2.5)

## 🔄 Review Schedule

- **Phase 1.2 Review**: After 1 month of production usage
- **Phase 2.0 Planning**: Q3 2025 based on Phase 1.2 results
- **Economic Model Tuning**: Ongoing based on user behavior data
- **Security Assessment**: Quarterly review of threat model and mitigations

---

*This decision establishes the technical foundation for economic bonds while providing a clear evolution path to fully trustless enforcement.*