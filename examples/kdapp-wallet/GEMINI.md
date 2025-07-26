# kdapp-wallet: Project Context

## Project Goal
Create a foundational, reusable CLI tool and daemon (`kdapp-wallet`) for the Kaspa ecosystem to securely manage user wallets and sign transactions.

## Core Architecture
- **Standalone Tool:** This is a new, separate project, not a submodule of `kaspa-auth` or other examples.
- **Daemon/CLI Model:** It will likely consist of a background daemon (`kdapp-walletd`) that holds keys and a CLI (`kdapp-wallet`) for user interaction.
- **OS Keychain Integration:** The daemon will store private keys securely in the native OS keychain (e.g., GNOME Keyring, KWallet), not in its own file. The key is unlocked on user login.

## Key Features
- Abstract away private key management from individual `kdapp` applications.
- Provide a secure, standardized way for applications to request signatures.
- Improve user experience by providing a single, system-wide wallet.

## Development Status (as of 2025-07-26)
- **Location:** `/mnt/c/Users/mariu/Documents/kdapp/kdapp/examples/kdapp-wallet`
- **Dependencies Added:**
    - `anyhow`
    - `clap` (with `derive` feature)
    - `tokio` (with `macros` and `rt-multi-thread` features)
- **Next Step:** Populate `src/main.rs` with the initial `clap` CLI structure for the commands: `create`, `address`, and `balance`.

Claude added this:
The transfer feature chaos
   we experienced shows exactly why we need
   a kdapp-wallet example as the
  fundamental building block.

  🎯 What We Experienced Without
  kdapp-wallet:

  ❌ The Problems We Hit:

  - Wallet draining bugs - Transfer
  functionality consuming entire balances
  - Extreme transaction fees - 389 KAS fees
   instead of ~0.25 KAS
  - UTXO conflicts - Bond transactions and
  comment transactions fighting over same
  UTXOs
  - Complex manual UTXO management - Having
   to track available vs locked UTXOs
  ourselves
  - Member ID resolution chaos - No clean
  way to transfer to "@034ed51a" style
  addresses

  🔧 What We Had to Build Manually:

  // We had to implement this ourselves in
  utxo_manager.rs
  pub struct UtxoLockManager {
      pub available_utxos:
  Vec<(TransactionOutpoint, UtxoEntry)>,
      pub locked_utxos: HashMap<u64,
  LockedUtxo>,
      pub total_available_balance: u64,
      pub total_locked_balance: u64,
      pub pending_bonds: HashMap<u64,
  String>,
      // ... tons of manual UTXO tracking
  }

  🚀 What kdapp-wallet Would Provide:

  ✅ Clean Transfer API:

  // What we SHOULD have been able to write
  let wallet = KdappWallet::new(keypair,
  network);
  let result =
  wallet.transfer_to_member("@034ed51a",
  amount_kas).await?;
  println!("✅ Transferred to {}: {}",
  result.recipient_address, result.tx_id);

  ✅ Built-in UTXO Management:

  // Instead of manual tracking, wallet
  handles this
  let balance =
  wallet.get_balance().await?; // Returns
  available vs locked
  let locked_funds =
  wallet.get_locked_utxos().await?; //
  Automatic tracking

  ✅ Member Resolution:

  // Clean member ID to address resolution
  let address =
  wallet.resolve_member_id("@034ed51a",
  episode_state)?;
  let transfer =
  wallet.create_transfer(address,
  amount).await?;

  ✅ Fee Estimation:

  // Proper fee calculation built-in
  let fee_estimate = wallet.estimate_transf
  er_fee(amount).await?;
  println!("Transfer will cost {:.6} KAS in
   fees", fee_estimate);

  💡 Missing Link Identified:

  The kdapp-wallet example would be the
  fundamental building block that
  comment-board (and all other kdapp
  examples) should build on top of. Instead
   of each example implementing its own
  wallet logic, they'd all use:

  // Clean, reusable wallet foundation
  use kdapp_wallet::{Wallet,
  TransferRequest, MemberResolver};

  // Comment-board just focuses on its core
   logic
  let wallet = Wallet::from_keypair(partici
  pant_keypair);
  let transfer =
  wallet.transfer_to_member(member_id,
  bond_amount).await?;

  🏗️ kdapp-wallet Example Should Provide:

  1. Clean Transfer Interface - No more
  manual UTXO juggling
  2. Member ID Resolution - Built-in
  "@member_id" to address mapping
  3. Fee Management - Proper fee estimation
   and handling
  4. Balance Tracking - Available vs locked
   fund management
  5. Transaction History - Built-in
  transaction tracking and confirmation
  6. Error Handling - Graceful handling of
  network issues, insufficient funds, etc.

  🎯 What This Means for comment-board:

  With kdapp-wallet as a foundation, our
  comment-board would be much simpler:

  // Instead of 300+ lines of UTXO
  management, just:
  let wallet = KdappWallet::new(keypair,
  network);

  // Clean bond creation
  let bond_tx =
  wallet.create_bond(comment_id,
  bond_amount, lock_duration).await?;

  // Clean member transfers
  let transfer_tx =
  wallet.transfer_to_member("@034ed51a",
  amount).await?;

  // Clean balance checking
  let balance =
  wallet.get_balance().await?;

  📋 Recommendation:

  Yes, kdapp-wallet should be the next
  priority! It would:

  ✅ Prevent the bugs we experienced -
  Proper UTXO management built-in✅ Enable
  elegant member transfers - Clean
  "@member_id" resolution✅ Simplify all
  future examples - Reusable wallet
  foundation✅ Reduce development
  complexity - Focus on business logic, not
   wallet mechanics✅ Improve security -
  Battle-tested wallet operations vs custom
   implementations

  The transfer feature chaos was a clear
  signal that we need this fundamental
  building block! 🎯