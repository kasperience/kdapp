# Quick Start Guide - kdapp MCP Server

This guide provides a quick way to get started with the kdapp MCP server and the TicTacToe AI game demo.

## Prerequisites

1. **Rust Toolchain**: Install Rust using [rustup](https://rustup.rs/)
2. **Python 3.7+**: For running the game coordinator
3. **Two AI Agents**:
   - One HTTP-based agent running at `http://127.0.0.1:1234`
   - Ollama with Gemma3 model: `ollama run gemma3`

## Quick Setup

### 1. Clone and Build

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

### 3. Verify AI Agents

Make sure both AI agents are running:
- HTTP agent at `http://127.0.0.1:1234`
- Ollama with Gemma3 model

## Running the TicTacToe Demo

### Option 1: Using the Coordinator Script

```bash
# Run the TicTacToe game coordinator
python tictactoe_coordinator.py
```

### Option 2: Using the Runner Scripts

**On Linux/Mac:**
```bash
./run_tictactoe_game.sh
```

**On Windows:**
```cmd
run_tictactoe_game.bat
```

## Direct Server Usage

### 1. Start the Server

```bash
cargo run --release
```

### 2. Send JSON-RPC Requests

Example tools list request:
```json
{"jsonrpc": "2.0", "id": 1, "method": "tools/list"}
```

Example tool call request:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "kdapp_start_episode",
    "arguments": {
      "participants": [
        "02a1b2c3d4e5f67890123456789012345678901234567890123456789012345678",
        "03b2c3d4e5f6789012345678901234567890123456789012345678901234567890"
      ]
    }
  }
}
```

## Key Components

### MCP Tools

1. **`kdapp_start_episode`**: Start a new multi-agent session
2. **`kdapp_execute_command`**: Execute commands within sessions
3. **`kdapp_get_episode_state`**: Retrieve session state
4. **`kdapp_generate_transaction`**: Generate transactions from commands

### TicTacToe Coordinator

The coordinator script (`tictactoe_coordinator.py`) demonstrates how to:
1. Initialize a game episode
2. Coordinate between two AI agents
3. Execute moves through the MCP server
4. Track game state and determine winners

## Troubleshooting

### Common Issues

1. **Server Won't Start**:
   - Ensure Rust is properly installed: `rustc --version`
   - Check for compilation errors: `cargo check`

2. **AI Agents Not Responding**:
   - Verify HTTP agent is running at `http://127.0.0.1:1234`
   - Check Ollama status: `ollama list`

3. **Python Dependencies**:
   - Install requests: `pip install requests`

### Getting Help

1. Check detailed documentation:
   - [README.md](README.md) - Main documentation
   - [KDAPP_MCP_TOOLS.md](KDAPP_MCP_TOOLS.md) - Tool documentation
   - [TICTACTOE_AI_GAME.md](TICTACTOE_AI_GAME.md) - Game implementation guide

2. View Rust documentation:
   ```bash
   cargo doc --open
   ```

## Next Steps

1. **Explore the Code**: Review `src/main.rs`, `src/tools.rs`, and `src/state.rs`
2. **Extend the Tools**: Add new MCP tools for additional functionality
3. **Implement New Games**: Use the framework for other kdapp games
4. **Enhance AI Prompts**: Improve the prompts for better gameplay
5. **Add Signatures**: Implement cryptographic signatures for moves

## Project Structure

```
kdapp-mcp-server/
├── src/                    # Rust source code
├── kdapp-modules/         # kdapp library modules
├── Cargo.toml             # Rust package manifest
├── tictactoe_coordinator.py # Game coordinator
├── README.md              # Main documentation
└── *.md                   # Additional documentation
```

## License

This project is licensed under the MIT License. See the LICENSE file for details.