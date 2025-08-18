# kdapp MCP Server Usage Guide

## Overview

This guide explains how to build, run, and interact with the kdapp MCP server. The server implements the Model Context Protocol (MCP) and provides tools for AI agents to interact with kdapp-based applications.

## Prerequisites

1. **Rust Toolchain**: Install Rust using [rustup](https://rustup.rs/)
2. **Cargo**: Included with Rust installation
3. **System Dependencies**: 
   - For Linux: May need to install `build-essential` or equivalent
   - For Windows: May need Visual Studio C++ Build tools
   - For macOS: Xcode command line tools

## Building the Server

### 1. Clone the Repository

```bash
cd /path/to/kdapp/examples/kdapp-mcp-server
```

### 2. Build the Project

```bash
# Development build
cargo build

# Release build (optimized)
cargo build --release
```

### 3. Check for Compilation Errors

```bash
# Check without building
cargo check

# Check with release profile
cargo check --release
```

## Running the Server

### 1. Direct Execution

```bash
# Run in development mode
cargo run

# Run in release mode
cargo run --release
```

### 2. Using the Built Binary

```bash
# After building, run the binary directly
./target/debug/kdapp-mcp-server

# Or for release build
./target/release/kdapp-mcp-server
```

## Interacting with the Server

The server communicates via stdin/stdout using JSON-RPC 2.0 protocol.

### 1. Tools List Request

To get a list of available tools:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/list"
}
```

### 2. Tool Call Request

To call a specific tool:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "kdapp_start_episode",
    "arguments": {
      "participants": ["pubkey1", "pubkey2"]
    }
  }
}
```

### 3. Example Session

Here's a complete example session:

```bash
# Start the server
cargo run

# Send tools list request
{"jsonrpc": "2.0", "id": 1, "method": "tools/list"}

# Server responds with tool list
# ... (server response) ...

# Start a new episode
{"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "kdapp_start_episode", "arguments": {"participants": ["02a1b2c3d4e5f67890123456789012345678901234567890123456789012345678", "03b2c3d4e5f6789012345678901234567890123456789012345678901234567890"]}}}

# Server responds with episode ID
# ... (server response) ...
```

## Available Tools

### kdapp_start_episode

Starts a new episode with specified participants.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "kdapp_start_episode",
    "arguments": {
      "participants": ["pubkey1", "pubkey2"]
    }
  }
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": "episode-12345"
}
```

### kdapp_execute_command

Executes a command in a specific episode.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "kdapp_execute_command",
    "arguments": {
      "episode_id": "episode-12345",
      "command": {"type": "move", "player": "X", "row": 1, "col": 1},
      "signature": "optional_signature"
    }
  }
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": null
}
```

### kdapp_get_episode_state

Retrieves the state of a specific episode.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "kdapp_get_episode_state",
    "arguments": {
      "episode_id": "episode-12345"
    }
  }
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "board": [[null, null, null], [null, "X", null], [null, null, null]],
    "current_player": "O"
  }
}
```

### kdapp_generate_transaction

Generates a transaction from a command.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "kdapp_generate_transaction",
    "arguments": {
      "command": {"type": "move", "player": "X", "row": 1, "col": 1}
    }
  }
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "transaction_id": "tx-67890",
    "payload": "serialized_command_data"
  }
}
```

## Error Handling

The server returns structured error responses:

**Parse Error:**
```json
{
  "jsonrpc": "2.0",
  "id": null,
  "error": {
    "code": -32700,
    "message": "Parse error",
    "data": "Detailed error information"
  }
}
```

**Method Not Found:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32601,
    "message": "Method not found"
  }
}
```

**Tool Execution Error:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32000,
    "message": "Tool execution error",
    "data": "Specific error details"
  }
}
```

## Testing

### Run Unit Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_start_episode
```

### Run Integration Tests

The server can be tested with manual JSON-RPC requests or automated scripts.

## Debugging

### 1. Enable Logging

Set the `RUST_LOG` environment variable:

```bash
# Enable info level logging
RUST_LOG=info cargo run

# Enable debug level logging
RUST_LOG=debug cargo run

# Enable module-specific logging
RUST_LOG=kaspa_mcp_server=debug cargo run
```

### 2. Use Debug Builds

Debug builds include additional checks and logging:

```bash
cargo build
# vs
cargo build --release
```

## Performance Considerations

### 1. Release Builds

For production use, always use release builds:

```bash
cargo run --release
```

### 2. Memory Usage

The server is designed to be memory efficient:
- Uses async/await for I/O operations
- Implements proper resource cleanup
- Maintains minimal state in memory

## Troubleshooting

### Common Issues

1. **Compilation Errors**
   - Ensure Rust toolchain is up to date: `rustup update`
   - Check for missing system dependencies

2. **Runtime Errors**
   - Check logs for detailed error messages
   - Verify JSON-RPC request format
   - Ensure proper tool parameters

3. **Connection Issues**
   - Verify stdin/stdout communication
   - Check for proper JSON formatting
   - Ensure newline termination of requests

### Getting Help

1. Check the documentation in this repository
2. Run `cargo doc --open` to view generated documentation
3. Use `cargo clippy` for code quality suggestions
4. File issues on the repository for bugs or feature requests

## Advanced Usage

### 1. Custom Episodes

To implement custom episodes:
1. Define your episode logic by implementing the `Episode` trait
2. Update the `ServerState` to use your episode type
3. Implement corresponding tool handlers

### 2. Extended Tool Set

To add new tools:
1. Add the tool definition to `handle_tools_list`
2. Implement the tool handler in `handle_tools_call`
3. Create the tool implementation in `tools.rs`

### 3. Performance Tuning

For high-performance scenarios:
1. Use release builds
2. Tune Tokio runtime settings
3. Optimize JSON serialization/deserialization
4. Profile with `cargo flamegraph` or similar tools

## Integration with AI Agents

The server is designed to work with any MCP-compatible AI agent. For specific integration guides:

1. **Claude Desktop**: Add to configuration as an MCP server
2. **Custom Agents**: Implement JSON-RPC 2.0 client
3. **Web Applications**: Use appropriate JSON-RPC libraries

For detailed integration examples, see the TicTacToe coordinator implementation.