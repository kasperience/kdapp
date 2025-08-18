# kdapp MCP Tools

This document describes the MCP tools implemented in the kdapp MCP server for coordinating multi-agent applications.

## Overview

The kdapp MCP server provides a set of tools that allow AI agents to interact with kdapp-based applications. These tools enable agents to:

1. Start and manage game episodes
2. Execute commands within episodes
3. Retrieve episode state
4. Generate transactions

## Available Tools

### 1. `kdapp_start_episode`

Starts a new episode with the specified participants.

**Parameters:**
- `participants` (required): Array of participant public keys

**Returns:**
- Episode ID as a string

**Example:**
```json
{
  "name": "kdapp_start_episode",
  "arguments": {
    "participants": [
      "02a1b2c3d4e5f67890123456789012345678901234567890123456789012345678",
      "03b2c3d4e5f6789012345678901234567890123456789012345678901234567890"
    ]
  }
}
```

### 2. `kdapp_execute_command`

Executes a command within a specific episode.

**Parameters:**
- `episode_id` (required): The ID of the episode
- `command` (required): The command to execute
- `signature` (optional): Cryptographic signature for the command

**Returns:**
- Success or error response

**Example:**
```json
{
  "name": "kdapp_execute_command",
  "arguments": {
    "episode_id": "12345",
    "command": {
      "type": "move",
      "player": "X",
      "row": 1,
      "col": 1
    },
    "signature": "3045022100..."
  }
}
```

### 3. `kdapp_get_episode_state`

Retrieves the current state of a specific episode.

**Parameters:**
- `episode_id` (required): The ID of the episode

**Returns:**
- Episode state as a JSON object

**Example:**
```json
{
  "name": "kdapp_get_episode_state",
  "arguments": {
    "episode_id": "12345"
  }
}
```

### 4. `kdapp_generate_transaction`

Generates a transaction from a command.

**Parameters:**
- `command` (required): The command to generate a transaction for

**Returns:**
- Transaction details as a JSON object

**Example:**
```json
{
  "name": "kdapp_generate_transaction",
  "arguments": {
    "command": {
      "type": "move",
      "player": "X",
      "row": 1,
      "col": 1
    }
  }
}
```

## Tool Categories

### Episode Management Tools
- `kdapp_start_episode`: Create new episodes
- `kdapp_get_episode_state`: Retrieve episode state

### Command Execution Tools
- `kdapp_execute_command`: Execute commands in episodes
- `kdapp_generate_transaction`: Generate transactions from commands

## Security Considerations

1. **Public Keys**: Participant identifiers are public keys
2. **Signatures**: Optional cryptographic signatures for command authentication
3. **Episode Isolation**: Each episode maintains its own state and participant list
4. **Command Validation**: The kdapp engine validates all commands before execution

## Error Handling

All tools return structured error responses with:
- Error code following JSON-RPC 2.0 standards
- Descriptive error message
- Additional context when available

## Integration with AI Agents

The kdapp MCP tools are designed to work seamlessly with AI agents by:

1. **Clear Input/Output**: Well-defined parameter structures and return values
2. **Stateless Operations**: Each tool call is self-contained
3. **Error Recovery**: Structured error responses allow agents to handle failures gracefully
4. **Extensibility**: The tool interface can be extended for new kdapp applications

## Example Workflow

1. **Start a TicTacToe game:**
```json
{
  "name": "kdapp_start_episode",
  "arguments": {
    "participants": ["player1_pubkey", "player2_pubkey"]
  }
}
```

2. **Execute a move:**
```json
{
  "name": "kdapp_execute_command",
  "arguments": {
    "episode_id": "game_123",
    "command": {
      "type": "move",
      "player": "X",
      "row": 1,
      "col": 1
    }
  }
}
```

3. **Check game state:**
```json
{
  "name": "kdapp_get_episode_state",
  "arguments": {
    "episode_id": "game_123"
  }
}
```

## Extending for New Applications

To extend the kdapp MCP server for new applications:

1. **Define New Commands**: Create new command structures for your application
2. **Implement Episode Logic**: Extend the Episode trait with your application's logic
3. **Add Tool Handlers**: Implement new tool handlers in the tools module
4. **Update Documentation**: Document the new tools in this file

The modular design of the kdapp MCP server makes it easy to add new applications while maintaining compatibility with existing AI agents.