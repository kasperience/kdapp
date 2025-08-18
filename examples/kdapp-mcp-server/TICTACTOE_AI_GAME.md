# TicTacToe AI Game Coordinator

This guide explains how to set up and run a TicTacToe game between two AI agents using the kdapp MCP server.

## Overview

Two AI agents play TicTacToe while the kdapp MCP server coordinates game state and enforces rules. The coordinator script runs the server (as a subprocess) and asks each agent for moves.

## Prerequisites

- kdapp MCP Server (Rust)
- Two AI agents:
  - HTTP agent at `http://127.0.0.1:1234` (LM Studio or similar)
  - Ollama running the `gemma3` model
- Python 3.7+
- Python package: `requests`

## Setup

1. Build/start the kdapp MCP server from the `kdapp-mcp-server` directory:

```bash
cargo run
```

The server listens for JSON-RPC requests on stdin/stdout.

2. Start both AI agents:

- HTTP agent: ensure it is available at `http://127.0.0.1:1234`
- Ollama: run the Gemma3 model

```bash
ollama run gemma3
```

3. Install Python dependencies:

```bash
pip install requests
```

## Coordinator: `tictactoe_coordinator.py`

Save the following Python script as `tictactoe_coordinator.py`. It demonstrates a simple coordinator that:

- Starts the kdapp MCP server as a subprocess
- Requests moves from two agents (HTTP + Ollama)
- Sends moves to the MCP server via JSON-RPC

```python
import json
import requests
import subprocess
import time

class TicTacToeCoordinator:
    def __init__(self):
        # Start the kdapp MCP server as a subprocess
        self.server_process = subprocess.Popen([
            'cargo', 'run'
        ], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)

        # AI Agent endpoints
        self.agent1_url = "http://127.0.0.1:1234"
        self.agent2_cmd = ["ollama", "run", "gemma3"]

        # Game state
        self.episode_id = None
        self.game_board = [[None for _ in range(3)] for _ in range(3)]
        self.current_player = "X"

    def send_mcp_request(self, method, params=None):
        """Send a request to the kdapp MCP server"""
        request = {"jsonrpc": "2.0", "id": 1, "method": method, "params": params or {}}
        self.server_process.stdin.write(json.dumps(request) + "\n")
        self.server_process.stdin.flush()
        response_line = self.server_process.stdout.readline()
        return json.loads(response_line.strip())

    def start_game(self, agent1_pubkey, agent2_pubkey):
        response = self.send_mcp_request("tools/call", {
            "name": "kdapp_start_episode",
            "arguments": {"participants": [agent1_pubkey, agent2_pubkey]}
        })
        if "result" in response:
            self.episode_id = response["result"]
            print(f"Game started with episode ID: {self.episode_id}")
            return True
        print(f"Error starting game: {response}")
        return False

    def execute_move(self, player, row, col):
        command = {"type": "move", "player": player, "row": row, "col": col}
        response = self.send_mcp_request("tools/call", {
            "name": "kdapp_execute_command",
            "arguments": {"episode_id": self.episode_id, "command": command}
        })
        return "result" in response

    def format_board_for_prompt(self):
        board_str = ""
        for i, row in enumerate(self.game_board):
            row_str = "|".join(cell or " " for cell in row)
            board_str += row_str + ("\n" if i < 2 else "")
            if i < 2:
                board_str += "-----\n"
        return board_str

    def get_default_move(self):
        for i in range(3):
            for j in range(3):
                if self.game_board[i][j] is None:
                    return i, j
        return 0, 0

    def update_board(self, row, col, player):
        self.game_board[row][col] = player

    def print_board(self):
        print("\nCurrent board:")
        for i, row in enumerate(self.game_board):
            print("|".join(cell or " " for cell in row))
            if i < 2:
                print("-----")

    def check_winner(self):
        for r in self.game_board:
            if r[0] == r[1] == r[2] and r[0] is not None:
                return r[0]
        for c in range(3):
            if self.game_board[0][c] == self.game_board[1][c] == self.game_board[2][c] and self.game_board[0][c] is not None:
                return self.game_board[0][c]
        if self.game_board[0][0] == self.game_board[1][1] == self.game_board[2][2] and self.game_board[0][0] is not None:
            return self.game_board[0][0]
        if self.game_board[0][2] == self.game_board[1][1] == self.game_board[2][0] and self.game_board[0][2] is not None:
            return self.game_board[0][2]
        if all(cell is not None for row in self.game_board for cell in row):
            return "Draw"
        return None

    def play_game(self):
        if not self.start_game("agent1_pubkey", "agent2_pubkey"):
            print("Failed to start game")
            return
        move_count = 0
        while move_count < 9:
            self.print_board()
            winner = self.check_winner()
            if winner:
                print("Game ended in a draw!" if winner == "Draw" else f"Player {winner} wins!")
                break
            # Fallback: use default move; replace with agent calls in production
            row, col = self.get_default_move()
            if self.execute_move(self.current_player, row, col):
                self.update_board(row, col, self.current_player)
                self.current_player = "O" if self.current_player == "X" else "X"
                move_count += 1
        self.print_board()
        self.server_process.terminate()

if __name__ == "__main__":
    coordinator = TicTacToeCoordinator()
    coordinator.play_game()
```

## Running the game

1. Make sure both AI agents are running:

- HTTP agent at `http://127.0.0.1:1234` (on the same host)
- Ollama with the `gemma3` model

2. Run the coordinator script:

```bash
python tictactoe_coordinator.py
```

## How it works

1. The coordinator starts a TicTacToe episode on the kdapp MCP server.
2. Agents take turns submitting moves.
3. Moves are validated/recorded by the MCP server.
4. The game ends on a win or draw.

## Customization

- Change the AI endpoints or models.
- Improve the prompts used to query agents.
- Add cryptographic signatures for move authenticity.
- Replace the fallback move logic with real agent calls and timeouts.

## Troubleshooting

- Server not starting: run `cargo build` and verify you're in the project directory.
- AI agents not responding: ensure they are reachable at the configured addresses/ports.
- Invalid moves: the MCP server enforces rules; the coordinator will retry or fall back to a default move.

## Extending to other games

To adapt this for another game, update the state representation, move format, and rule enforcement logic; the MCP server provides the coordination layer.

        self.server_process.terminate()

if __name__ == "__main__":
    coordinator = TicTacToeCoordinator()
    coordinator.play_game()

## Running the game

1. Ensure both AI agents are running (HTTP agent at `http://127.0.0.1:1234`, Ollama with `gemma3`).

2. Run the coordinator:

```bash
python tictactoe_coordinator.py
```

## How it works

1. Coordinator starts an episode on the kdapp MCP server.
2. Agents take turns producing moves.
3. Moves are validated and applied through the MCP server.
4. The game ends on win or draw.

## Customization

- Change AI endpoints or models.
- Improve prompts for better move outputs.
- Add cryptographic signatures for moves.
- Add a nicer UI or time limits per move.

## Troubleshooting

- Server not starting: run `cargo build` and check current directory.
- AI agents not responding: verify the agents are reachable on their ports.
- Invalid moves: the MCP server enforces rules; coordinator falls back to a default move.

## Extending to other games

To adapt this to other games:

- Change the game state representation.
- Update the move format and validation logic.
- Adjust AI prompts for the new game's rules.

The kdapp MCP server provides flexible state management and enforcement for multi-agent games.
Note: Since you're running the agents on Windows and the coordinator in WSL, the localhost addresses will still work because WSL provides seamless network integration with Windows.

### 3. Install Python Dependencies

```bash
pip install requests
```

## Game Coordinator Implementation

Create a file named `tictactoe_coordinator.py`:

```python
import json
import requests
import subprocess
import sys
import time

class TicTacToeCoordinator:
    def __init__(self):
        # Start the kdapp MCP server as a subprocess
        self.server_process = subprocess.Popen(
            ['cargo', 'run'],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1
        )
        
        # AI Agent endpoints
        self.agent1_url = "http://127.0.0.1:1234"
        self.agent2_cmd = ["ollama", "run", "gemma3"]
        
        # Game state
        self.episode_id = None
        self.game_board = [[None for _ in range(3)] for _ in range(3)]
        self.current_player = "X"
        
    def send_mcp_request(self, method, params=None):
        """Send a request to the kdapp MCP server"""
        request = {
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params or {}
        }
        
        # Send request to server
        self.server_process.stdin.write(json.dumps(request) + "\n")
        self.server_process.stdin.flush()
        
        # Read response
        response_line = self.server_process.stdout.readline()
        return json.loads(response_line.strip())
    
    def start_game(self, agent1_pubkey, agent2_pubkey):
        """Start a new TicTacToe game"""
        response = self.send_mcp_request("tools/call", {
            "name": "kdapp_start_episode",
            "arguments": {
                "participants": [agent1_pubkey, agent2_pubkey]
            }
        })
        
        if "result" in response:
            self.episode_id = response["result"]
            print(f"Game started with episode ID: {self.episode_id}")
            return True
        else:
            print(f"Error starting game: {response}")
            return False
    
    def get_game_state(self):
        """Get the current game state"""
        response = self.send_mcp_request("tools/call", {
            "name": "kdapp_get_episode_state",
            "arguments": {
                "episode_id": self.episode_id
            }
        })
        
        if "result" in response:
            return response["result"]
        else:
            print(f"Error getting game state: {response}")
            return None
    
    def execute_move(self, player, row, col):
        """Execute a move in the game"""
        command = {
            "type": "move",
            "player": player,
            "row": row,
            "col": col
        }
        
        response = self.send_mcp_request("tools/call", {
            "name": "kdapp_execute_command",
            "arguments": {
                "episode_id": self.episode_id,
                "command": command
            }
        })
        
        return "result" in response
    
    def get_move_from_agent1(self, game_state):
        """Get a move from the HTTP AI agent"""
        prompt = f"""
        You are playing TicTacToe. You are 'X'.
        Current board state:
        {self.format_board_for_prompt()}
        
        Please provide your move as a JSON object with "row" and "col" fields (0-2).
        Example: {{"row": 1, "col": 1}}
        """
        
        try:
            response = requests.post(
                f"{self.agent1_url}/v1/chat/completions",
                json={
                    "model": "gemma3",
                    "messages": [{"role": "user", "content": prompt}],
                    "temperature": 0.7
                }
            )
            
            if response.status_code == 200:
                content = response.json()["choices"][0]["message"]["content"]
                # Try to extract JSON from the response
                import re
                json_match = re.search(r'\{.*\}', content)
                if json_match:
                    move = json.loads(json_match.group())
                    return move["row"], move["col"]
        except Exception as e:
            print(f"Error getting move from agent 1: {e}")
        
        # Default move if there's an error
        return self.get_default_move()
    
    def get_move_from_agent2(self, game_state):
        """Get a move from the Ollama AI agent"""
        prompt = f"""
        You are playing TicTacToe. You are 'O'.
        Current board state:
        {self.format_board_for_prompt()}
        
        Please provide your move as a JSON object with "row" and "col" fields (0-2).
        Example: {{"row": 1, "col": 1}}
        """
        
        try:
            # For Ollama, we need to run the command and capture output
            process = subprocess.Popen(
                self.agent2_cmd,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True
            )
            
            stdout, stderr = process.communicate(input=prompt)
            
            if process.returncode == 0:
                # Try to extract JSON from the response
                import re
                json_match = re.search(r'\{.*\}', stdout)
                if json_match:
                    move = json.loads(json_match.group())
                    return move["row"], move["col"]
        except Exception as e:
            print(f"Error getting move from agent 2: {e}")
        
        # Default move if there's an error
        return self.get_default_move()
    
    def format_board_for_prompt(self):
        """Format the board for the AI prompt"""
        board_str = ""
        for i, row in enumerate(self.game_board):
            row_str = ""
            for j, cell in enumerate(row):
                if cell is None:
                    row_str += " "
                else:
                    row_str += cell
                if j < 2:
                    row_str += "|"
            board_str += row_str + "\n"
            if i < 2:
                board_str += "-----\n"
        return board_str
    
    def get_default_move(self):
        """Get a default move when AI fails"""
        for i in range(3):
            for j in range(3):
                if self.game_board[i][j] is None:
                    return i, j
        return 0, 0
    
    def update_board(self, row, col, player):
        """Update the local board state"""
        self.game_board[row][col] = player
    
    def print_board(self):
        """Print the current board state"""
        print("\nCurrent board:")
        for i, row in enumerate(self.game_board):
            row_str = ""
            for j, cell in enumerate(row):
                if cell is None:
                    row_str += " "
                else:
                    row_str += cell
                if j < 2:
                    row_str += "|"
            print(row_str)
            if i < 2:
                print("-----")
        print()
    
    def check_winner(self):
        """Check if there's a winner"""
        # Check rows
        for row in self.game_board:
            if row[0] == row[1] == row[2] and row[0] is not None:
                return row[0]
        
        # Check columns
        for col in range(3):
            if (self.game_board[0][col] == self.game_board[1][col] == 
                self.game_board[2][col] and self.game_board[0][col] is not None):
                return self.game_board[0][col]
        
        # Check diagonals
        if (self.game_board[0][0] == self.game_board[1][1] == 
            self.game_board[2][2] and self.game_board[0][0] is not None):
            return self.game_board[0][0]
        
        if (self.game_board[0][2] == self.game_board[1][1] == 
            self.game_board[2][0] and self.game_board[0][2] is not None):
            return self.game_board[0][2]
        
        # Check for draw
        is_full = all(cell is not None for row in self.game_board for cell in row)
        if is_full:
            return "Draw"
        
        return None
    
    def play_game(self):
        """Main game loop"""
        # Start the game with dummy public keys
        if not self.start_game("agent1_pubkey", "agent2_pubkey"):
            print("Failed to start game")
            return
        
        # Game loop
        move_count = 0
        while move_count < 9:
            self.print_board()
            
            # Check for winner
            winner = self.check_winner()
            if winner:
                if winner == "Draw":
                    print("Game ended in a draw!")
                else:
                    print(f"Player {winner} wins!")
                break
            
            # Get move from current player
            if self.current_player == "X":
                print("Agent 1's turn (X)")
                row, col = self.get_move_from_agent1(self.game_board)
            else:
                print("Agent 2's turn (O)")
                row, col = self.get_move_from_agent2(self.game_board)
            
            # Execute the move
            if self.execute_move(self.current_player, row, col):
                print(f"Player {self.current_player} moves to ({row}, {col})")
                self.update_board(row, col, self.current_player)
                
                # Switch players
                self.current_player = "O" if self.current_player == "X" else "X"
                move_count += 1
            else:
                print("Invalid move, trying again...")
                # Get a default move
                row, col = self.get_default_move()
                if self.execute_move(self.current_player, row, col):
                    print(f"Player {self.current_player} moves to ({row}, {col})")
                    self.update_board(row, col, self.current_player)
                    self.current_player = "O" if self.current_player == "X" else "X"
                    move_count += 1
        
        # Final board state
        self.print_board()
        
        # Close the server process
        self.server_process.terminate()

if __name__ == "__main__":
    coordinator = TicTacToeCoordinator()
    coordinator.play_game()
```

## Running the Game

1. Make sure both AI agents are running:
   - HTTP agent at `http://127.0.0.1:1234` (on Windows)
   - Ollama with Gemma3 model (on Windows or WSL)

2. Run the coordinator from WSL:
   ```bash
   python3 tictactoe_coordinator.py
   ```

3. Watch as the two AI agents play TicTacToe against each other!

Note: Network communication between Windows and WSL works seamlessly for localhost addresses, so your Windows-based AI agents will be accessible from the WSL environment.

## How It Works

1. **Game Initialization**: The coordinator starts a new TicTacToe episode using the kdapp MCP server
2. **Turn-based Play**: Agents take turns making moves
3. **State Management**: The kdapp server maintains the official game state and enforces rules
4. **Move Execution**: Each agent's move is executed through the MCP server
5. **Game End**: The game continues until there's a winner or draw

## Customization

You can customize the game by:

1. **Changing AI Models**: Modify the endpoints to use different AI models
2. **Adjusting Prompts**: Improve the prompts to get better moves from the AI agents
3. **Adding Signatures**: Implement proper cryptographic signatures for moves
4. **Enhancing UI**: Improve the board visualization
5. **Adding Time Limits**: Implement timeouts for AI moves

## Troubleshooting

1. **Server Not Starting**: Ensure you're in the correct directory and have built the project with `cargo build`
2. **AI Agents Not Responding**: Verify both agents are running and accessible
3. **Invalid Moves**: The kdapp server will reject invalid moves, and the coordinator will request a new move
4. **Connection Issues**: Check that all services are running on the correct ports

## Extending to Other Games

This framework can be extended to other kdapp games by:

1. Modifying the game state representation
2. Changing the move format
3. Updating the rule enforcement logic
4. Adjusting the AI prompts for the new game

The kdapp MCP server provides a flexible foundation for multi-agent games with proper state management and rule enforcement.