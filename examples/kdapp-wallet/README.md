# kdapp-wallet: Kaspa CLI Wallet

`kdapp-wallet` is a foundational, reusable command-line interface (CLI) tool for the Kaspa ecosystem designed to securely manage user wallets and sign transactions. It aims to abstract away private key management from individual `kdapp` applications, providing a secure and standardized way for applications to interact with user funds.

## Project Goal

To create a robust CLI tool that:
- Securely manages Kaspa private keys using the native OS keychain.
- Provides a simple interface for common wallet operations like creating a wallet, retrieving an address, and checking the balance.
- Serves as a building block for other `kdapp` examples, preventing redundant wallet implementations.

## Core Architecture

-   **Standalone Tool:** `kdapp-wallet` is a new, independent project.
-   **CLI Model:** It currently operates as a CLI tool, with future plans for a background daemon (`kdapp-walletd`) to hold keys.
-   **OS Keychain Integration:** Private keys are stored securely in the native OS keychain (e.g., Windows Credential Manager, GNOME Keyring, KWallet), leveraging system-level security features.

## Key Features

-   **Secure Key Storage:** Utilizes OS-native keychain for private key management.
-   **Address Derivation:** Derives Kaspa addresses from stored keys.
-   **Balance Inquiry:** Connects to a Kaspa node to query wallet balance.
-   **Real Cryptography:** Uses `secp256k1` for key generation and `kaspacore` for cryptographic operations.

## Getting Started

### Prerequisites

-   **Rust & Cargo:** Ensure you have Rust and Cargo installed. You can install them via `rustup`: `https://rustup.rs/`
-   **Kaspa Node:** For the `balance` command to function, you need a running Kaspa node accessible via gRPC (default: `grpc://127.0.0.1:16110`). You can download `kaspad` from the official Kaspa GitHub releases.

### Windows-Specific Setup

If you are building and running `kdapp-wallet` on Windows, you **must** enable the `windows-native` feature for the `keyring` crate in your `Cargo.toml`. Your `[dependencies]` section should look like this:

```toml
[dependencies]
keyring = { version = "2.0.0", features = ["windows-native"] }
# ... other dependencies
```

### Building the Project

Navigate to the `kdapp-wallet` project directory in your terminal and build the project:

```bash
cargo build
```

### CLI Commands

You can run the commands using `cargo run -- <command> [arguments]`.

#### 1. `create` - Create a new wallet

Generates a new Kaspa keypair and securely stores the private key. By default, it uses the OS keychain. For development, you can store it in a local file.

```bash
cargo run -- create [OPTIONS]
```

**Options:**

-   `--dev-mode`: **(INSECURE! DO NOT USE FOR REAL FUNDS!)** Stores the private key in a local file named `.kdapp-wallet-dev-key` within the project directory. This is for development purposes only.

**Example Usage:**

```bash
# Create a wallet securely in the OS keychain (default)
cargo run -- create

# Create a wallet in development mode (key stored in file)
cargo run -- create --dev-mode
```

**Example Output (Secure Mode):**
```
Generating new wallet...
Wallet created and stored securely in OS keychain.
```

**Example Output (Dev Mode):**
```
Generating new wallet...

WARNING: Development mode enabled. Private key will be stored INSECURELY in a local file.
DO NOT USE FOR REAL FUNDS!

Wallet created and private key stored in '.kdapp-wallet-dev-key'.

WALLET NEEDS FUNDING! Visit https://faucet.kaspanet.io/ and fund: kaspatest:...
```

#### 2. `address` - Get wallet address

Retrieves the private key and derives the corresponding Kaspa public address.

```bash
cargo run -- address [OPTIONS]
```

**Options:**

-   `--dev-mode`: Use development mode (read key from `.kdapp-wallet-dev-key` file).

**Example Usage:**

```bash
# Get address from securely stored key
cargo run -- address

# Get address from key stored in dev mode file
cargo run -- address --dev-mode
```

**Example Output:**
```
Retrieving wallet address...
Wallet Address: kaspa:qq20...
```

#### 3. `balance` - Get wallet balance

Retrieves the wallet address and connects to a Kaspa node to query the current balance.

```bash
cargo run -- balance [OPTIONS]
```

**Options:**

-   `--rpc-url <URL>`: Optional. The gRPC URL of the Kaspa node to connect to (e.g., `"grpc://your.kaspa.node:16110"`). If not provided, it defaults to `grpc://127.0.0.1:16110`.
-   `--dev-mode`: Use development mode (read key from `.kdapp-wallet-dev-key` file).

**Example Usage:**

```bash
# Using the default local node and securely stored key
cargo run -- balance

# Connecting to a specific remote node with key from dev mode file
cargo run -- balance --rpc-url "grpc://some.public.node:16110" --dev-mode
```

**Example Output:**
```
Getting wallet balance...
Wallet Balance: 123.456789 KAS
```

## Development Status

This project is currently under active development. For more detailed architectural decisions and development guidelines, please refer to the `GEMINI.md` file in this directory.