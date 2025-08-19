# kdapp MCP Server & TicTacToe AI Game

This repository contains a Rust-based Model Context Protocol (MCP) server that enables AI agents to interact with kdapp-based applications, plus a complete implementation of a TicTacToe game coordinator that allows two AI agents to play against each other.

## 🎉 SUCCESS: AI vs AI TicTacToe Game Working!

As demonstrated in our testing, two different AI agents can successfully play TicTacToe against each other:
- **Agent 1**: LM Studio with Gemma model (port 1234)
- **Agent 2**: Ollama with Gemma3 model (port 11434)

The game coordinator successfully:
1. Starts a new game episode
2. Coordinates turns between both agents
3. Processes moves through the kdapp MCP server
4. Tracks game state and detects wins/draws

## 🔐 REAL WALLET FUNCTIONALITY NOW IMPLEMENTED!

Our latest update adds genuine wallet management capabilities:
- **Real keypair generation** using OS-provided cryptographically secure randomness
- **Persistent wallet storage** with separate files for different agents
- **Valid Kaspa addresses** for testnet-10 network
- **Funding guidance** with links to the official Kaspa testnet faucet
- **Automatic wallet loading** - creates new wallets on first run, loads existing ones afterward

## 🚀 Features

### kdapp MCP Server
- **Language**: Pure Rust for performance and safety
- **Protocol**: JSON-RPC 2.0 compliant
- **Communication**: stdin/stdout interface for MCP compatibility
- **Integration**: Direct integration with kdapp modules
- **Tools**: Four core MCP tools for AI agent interaction
- **Wallet Management**: Real wallet creation, loading, and persistence

### TicTacToe AI Game Coordinator
- **Multi-Agent Support**: Works with different AI services simultaneously
- **Cross-Platform**: Compatible with Windows-based AI agents and WSL server
- **Automatic Gameplay**: Full game initialization, move coordination, and win detection
- **Error Handling**: Graceful fallback when agents are unavailable
- **Real-time Display**: Visual board updates during gameplay

## 📋 Prerequisites

1. **Rust Toolchain**: Install Rust using [rustup](https://rustup.rs/)
2. **Python 3.7+**: For running the game coordinator
3. **Two AI Agents** (local, small models OK):
   - LM Studio chat server on `http://127.0.0.1:1234`
   - Ollama with `gemma3:270m` on `http://127.0.0.1:11434`
   - See Local Agents guide: AI_AGENTS.md

## 🛠️ Setup Instructions

### 1. Build the kdapp MCP Server

```bash
# Navigate to the project directory
cd /path/to/kdapp/examples/kdapp-mcp-server

# Build the project
cargo build --release
```

### 2. Install Python Dependencies

```bash
pip install requests
```

### 3. Start AI Agents

- LM Studio: enable local server (port 1234) and load a small instruct chat model
- Ollama: `ollama pull gemma3:270m` then `ollama run gemma3:270m`
  
Details and troubleshooting: AI_AGENTS.md

## ▶️ Running the TicTacToe Demo

### Option 1: Using Python directly
```bash
python tictactoe_coordinator.py
```

### Option 2: Using the runner scripts
```bash
# On Windows
run_tictactoe_game.bat

# On Linux/Mac
./run_tictactoe_game.sh
```

## 🔐 Wallet Management

The kdapp MCP server now includes genuine wallet management capabilities:

### Wallet Features
- **Automatic Wallet Creation**: Creates new wallets on first run
- **Persistent Storage**: Saves wallets to `agent_keys/` directory
- **Separate Agent Wallets**: Different files for different agents
- **Real Cryptography**: Uses OS-provided secure randomness
- **Valid Addresses**: Generates proper Kaspa testnet-10 addresses

### Wallet Files
- `agent_keys/agent1-wallet.key` - Wallet for Agent 1
- `agent_keys/agent2-wallet.key` - Wallet for Agent 2
- Keys are stored in binary format (32-byte secret keys)

### Funding Your Wallets
After wallet creation, you'll see funding addresses and links to the Kaspa testnet faucet:
```bash
💰 Funding Address: kaspatest:qrzsau3684ag9cvvxaxtsagn72r7l8xnu9ne8jp4e2l26q5a8qr25quvdc55v
🌐 Network: testnet-10
💡 Fund this address at: https://faucet.kaspanet.io/
```

## 🎮 How It Works

1. **Game Initialization**: The coordinator starts a new TicTacToe episode using the kdapp MCP server
2. **Agent Coordination**: Players take turns making moves through their respective AI services
3. **Move Processing**: Each agent's move is processed through the MCP server for validation
4. **State Management**: The kdapp engine maintains official game state and enforces rules
5. **Game Completion**: The game continues until there's a winner or draw

## 🧠 MCP Tools

The kdapp MCP server provides four core tools for AI agent interaction:

### 1. `kdapp_start_episode`
Starts a new multi-agent session with specified participants.

### 2. `kdapp_execute_command`
Executes commands within game sessions with optional cryptographic signatures.

### 3. `kdapp_get_episode_state`
Retrieves the current state of a game session.

### 4. `kdapp_generate_transaction`
Generates blockchain transactions from game commands.

## 📁 Project Structure

```
kdapp-mcp-server/
├── src/                    # Rust source code
│   ├── main.rs             # Minimal entrypoint (calls app::run)
│   ├── app.rs              # Startup + JSON-RPC loop
│   ├── rpc_handlers.rs     # MCP tool dispatch
│   ├── jsonrpc.rs          # JSON-RPC types
│   ├── state.rs            # Engine + episode state and persistence
│   ├── tools.rs            # MCP tool implementations
│   ├── wallet.rs           # Wallet management
│   └── bin/
│       ├── test_wallet.rs  # Wallet test utility
│       └── test_node.rs    # Node connectivity test
├── agent_keys/            # Wallet storage directory
├── episodes/              # Local episode snapshots (git-ignored)
├── Cargo.toml             # Rust package manifest
├── AI_AGENTS.md           # Local AI agents setup guide
├── tictactoe_coordinator.py # Game coordinator script
├── run_tictactoe_game.sh  # Linux/Mac runner script
├── run_tictactoe_game.bat # Windows runner script
├── README.md              # This documentation
├── KDAPP_MCP_TOOLS.md     # Detailed tool documentation
├── KDAPP_SERVER_USAGE.md  # Server usage guide
├── TICTACTOE_AI_GAME.md   # Game implementation guide
├── QUICK_START.md         # Quick start guide
├── PROJECT_SUMMARY.md     # Project summary
└── LICENSE                # License file
```

## 📦 kdapp Modules Management

This project now directly uses the `kdapp` modules from the main kdapp workspace through a path dependency. This approach provides:

1. **Always Up-to-Date**: The project automatically uses the latest version of kdapp modules from the workspace
2. **Simplified Management**: No need to maintain local copies or synchronization scripts
3. **Consistent Development**: Ensures compatibility with the main kdapp project development

### Updating kdapp Modules

To update the kdapp modules to a newer version:

1. **Update the main kdapp workspace**: Pull the latest changes from the main kdapp repository
2. Run `cargo build` to ensure everything compiles correctly with the updated modules

## 🔧 Troubleshooting

### Common Issues

1. **Connection Refused Errors**:
   - Ensure both AI agents are running
   - Check firewall settings for ports 1234 and 11434
   - Enable CORS in LM Studio if needed
   - Verify network binding (0.0.0.0 vs 127.0.0.1)

2. **Python Not Found**:
   - Install Python from https://www.python.org/downloads/
   - Add Python to PATH during installation
   - Install required packages: `pip install requests`

3. **Default Moves**:
   - If agents can't be reached, the game continues with default moves
   - Check agent URLs and model names in coordinator script

### Network Configuration (WSL/Windows)

When running from WSL with Windows-based agents:
1. Find Windows host IP: `cat /etc/resolv.conf | grep nameserver`
2. Coordinator auto-detects host IP; if needed, set the Windows host IP manually or use 127.0.0.1

## 🌟 Future Enhancements

1. **On-Chain Transaction Integration**: Connect wallet functionality to actual blockchain transactions
2. **Advanced Game Logic**: Implement more complex kdapp games
3. **Improved AI Prompts**: Better prompts for more strategic gameplay
4. **Web Dashboard**: Real-time game monitoring and visualization
5. **Tournament Mode**: Multiple games with scoring and statistics

## 📄 License

This project is licensed under the MIT License - see the LICENSE file for details.

## 🙏 Acknowledgments

- kdapp framework for the core multi-agent application infrastructure
- Rust community for the excellent tooling and ecosystem
- AI model providers (Google Gemma, etc.) for the underlying intelligence
