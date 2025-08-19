import json
import requests
import subprocess
import sys
import time
import os
from datetime import datetime
import secrets
import hashlib

class TicTacToeCoordinator:
    def __init__(self):
        # Start the kdapp MCP server as a subprocess
        self.server_process = subprocess.Popen(
            ['cargo', 'run', '--bin', 'kdapp-mcp-server'],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            # Merge stderr into stdout so we don't block waiting on the wrong stream
            stderr=subprocess.STDOUT,
            text=True,
            encoding='utf-8',
            bufsize=1
        )

        # Wait for the server to be ready before sending requests
        self.log('INFO', 'Waiting for kdapp MCP server to start...')
        start_time = time.time()
        ready = False
        offline_mode_detected = False
        # Accept multiple readiness phrases (case-insensitive), on either stream
        readiness_markers = (
            'connected to kaspa node successfully',  # main.rs
            'successfully connected to kaspa node',  # node_connector.rs
            'continues in offline mode',             # main.rs warning text
            'server continues in offline mode',      # variant
            'finished dev',                          # cargo finished build
            'running `target',                        # cargo running binary
        )
        while time.time() - start_time < 90:  # up to 90-second timeout to allow first build
            line = self.server_process.stdout.readline()
            if line:
                stripped = line.strip()
                # Mirror all startup output to stderr for visibility
                print(f"[Server Startup]: {stripped}", file=sys.stderr)
                low = stripped.lower()
                if any(marker in low for marker in readiness_markers):
                    # Consider server ready once we see any known marker
                    self.log('INFO', 'Server is ready.')
                    if 'offline mode' in low:
                        offline_mode_detected = True
                    ready = True
                    break
            else:
                # Avoid a busy-loop when there's no output available
                time.sleep(0.05)
        
        if not ready:
            self.log('ERROR', 'Timeout waiting for server to start.')
            # Dump any remaining buffered output to aid debugging
            try:
                remaining = self.server_process.stdout.read() or ''
                if remaining:
                    print(f"[Server Startup - tail]:\n{remaining}", file=sys.stderr)
            except Exception:
                pass
            raise RuntimeError("kdapp MCP server failed to start in time.")

        # Query server agent pubkeys for proper participant authorization
        try:
            pk_resp = self.send_mcp_request("tools/call", {"name": "kdapp_get_agent_pubkeys"})
            if isinstance(pk_resp, dict) and isinstance(pk_resp.get('result'), dict):
                self.agent1_pubkey = pk_resp['result'].get('agent1_pubkey')
                self.agent2_pubkey = pk_resp['result'].get('agent2_pubkey')
                self.log('INFO', f"Server wallets pubkeys loaded: agent1={self.agent1_pubkey}, agent2={self.agent2_pubkey}")
            else:
                self.agent1_pubkey = None
                self.agent2_pubkey = None
        except Exception as e:
            self.agent1_pubkey = None
            self.agent2_pubkey = None
            self.log('WARN', f'Could not fetch server agent pubkeys: {e}')
        
        # AI Agent endpoints
        # Agent 1: LM Studio on port 1234 (Windows host from WSL)
        # Try to get the Windows host IP, fallback to localhost
        windows_host = self.get_windows_host_ip()
        self.agent1_url = f"http://{windows_host}:1234"
        # Agent 2: Ollama on port 11434
        self.agent2_url = f"http://{windows_host}:11434"
        
        print(f"Agent 1 (LM Studio) URL: {self.agent1_url}")
        print(f"Agent 2 (Ollama) URL: {self.agent2_url}")
        
        # On-chain enforcement (require real txs)
        # Auto-disable when server started in offline mode
        self.require_onchain = not offline_mode_detected
        self.signer_address = None  # Not needed - server handles signing
        self.node_rpc_url = None    # Not needed - server is already connected
        
        # Game state
        self.episode_id = None
        self.game_board = [[None for _ in range(3)] for _ in range(3)]
        self.current_player = "X"

        # Agent key directories and addresses
        self.agent_keys_root = os.path.join(os.getcwd(), 'agent_keys')
        self.agent1_folder = os.path.join(self.agent_keys_root, 'agent1')
        self.agent2_folder = os.path.join(self.agent_keys_root, 'agent2')
        self._ensure_agent_key_dirs()
        self.agent1_addr, self.agent1_keyfile = self._load_or_create_agent(self.agent1_folder)
        self.agent2_addr, self.agent2_keyfile = self._load_or_create_agent(self.agent2_folder)
        self.log('INFO', f'Agent1 address: {self.agent1_addr}, keyfile: {self.agent1_keyfile}')
        self.log('INFO', f'Agent2 address: {self.agent2_addr}, keyfile: {self.agent2_keyfile}')
        
    def get_windows_host_ip(self):
        """Get the Windows host IP from WSL"""
        try:
            # Try to get the nameserver IP from resolv.conf
            with open('/etc/resolv.conf', 'r') as f:
                for line in f:
                    if line.startswith('nameserver'):
                        return line.split()[1]
        except:
            pass
        
        # Fallback to localhost
        return '127.0.0.1'

    def _ensure_agent_key_dirs(self):
        os.makedirs(self.agent1_folder, exist_ok=True)
        os.makedirs(self.agent2_folder, exist_ok=True)

    def _load_or_create_agent(self, folder):
        """Load an agent key from the real wallet files. Returns (address, keyfile_path)."""
        # Use the real wallet key files instead of demo keys
        agent_name = "agent1" if "agent1" in folder else "agent2"
        keyfile = os.path.join(os.getcwd(), f'agent_keys/{agent_name}-wallet.key')
        
        if os.path.exists(keyfile):
            try:
                # Read the binary key file
                with open(keyfile, 'rb') as f:
                    key_data = f.read()
                    if len(key_data) == 32:
                        # Generate the public key and address from the secret key
                        # For now, we'll use a placeholder address format
                        # In a real implementation, we'd derive the actual Kaspa address
                        addr = f"real_{agent_name}_{keyfile.split(os.sep)[-1].split('-')[0]}"
                        return addr, keyfile
            except Exception as e:
                print(f"Error loading real key file: {e}")
                pass

        # Fallback to demo keys if real keys aren't available
        keyfile = os.path.join(folder, 'key.json')
        if os.path.exists(keyfile):
            try:
                with open(keyfile, 'r') as f:
                    data = json.load(f)
                    return data.get('address'), keyfile
            except Exception:
                pass

        # create new demo key
        priv = secrets.token_hex(32)
        # derive a pseudo-address for demo: sha256 of key, hex prefix
        addr = 'demo_' + hashlib.sha256(priv.encode()).hexdigest()[:40]
        data = {'address': addr, 'private_key_demo': priv}
        with open(keyfile, 'w') as f:
            json.dump(data, f)
        return addr, keyfile
        
    def has_onchain_capability(self):
        """Return True if the server can handle on-chain transactions."""
        # The server is already connected to the node and handles signing
        return True

    def extract_tx_hash(self, response):
        """Try to extract a transaction hash/id from the server response result.
        Returns tx_hash string or None.
        """
        try:
            if not isinstance(response, dict):
                return None
            result = response.get('result')
            if not result:
                return None
            # result may be dict or string; if dict, check common fields
            if isinstance(result, dict):
                for key in ('tx_hash', 'txid', 'tx', 'transaction', 'hash'):
                    if key in result and result[key]:
                        return result[key]
                # sometimes nested
                if 'receipt' in result and isinstance(result['receipt'], dict):
                    for key in ('tx_hash', 'txid', 'hash'):
                        if key in result['receipt']:
                            return result['receipt'][key]
            # if result is a string that looks like a hash, return it
            if isinstance(result, str) and len(result) >= 8:
                return result
        except Exception:
            return None
        return None
    
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
    
    def execute_move(self, player, row, col, signer):
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
                "command": command,
                "signer": signer
            }
        })
        # Log full server response for audit and confirmation
        try:
            self.log('INFO', f'execute_move response: {response}')
        except Exception:
            pass

        # Consider the move successful when we get a result and no error
        if isinstance(response, dict) and 'error' not in response and 'result' in response:
            # If result is explicitly None, warn the user this may be an episode-only action
            if response.get('result') is None:
                self.log('WARN', 'kdapp_execute_command returned null result — this may indicate no on-chain transaction was produced')
                if self.require_onchain:
                    self.log('ERROR', 'On-chain enforcement is enabled and kdapp did not return tx metadata; rejecting move')
                    return False

            # If on-chain required, verify tx metadata exists
            if self.require_onchain:
                tx_hash = self.extract_tx_hash(response)
                if not tx_hash:
                    self.log('ERROR', 'On-chain enforcement enabled but no tx hash found in response; rejecting move')
                    return False
                self.log('INFO', f'Move produced on-chain tx: {tx_hash}')

            return True
        return False

    def log(self, level, message):
        """Simple timestamped logger"""
        ts = datetime.now().strftime('%Y-%m-%d %H:%M:%S')
        print(f"{ts} [{level}] {message}")
    
    def get_move_from_agent1(self, game_state):
        """Get a move from the HTTP AI agent (LM Studio)"""
        prompt = f"""
You are playing TicTacToe as 'X'. Analyze the current board state and make a strategic move.

Current board state:
{self.format_board_for_prompt()}

Analyze the board carefully:
1. Look for any possible winning moves for you (X)
2. Look for any moves your opponent (O) needs to be blocked
3. Consider strategic positions (center, corners)
4. Think about your next move strategically

Provide your move as a JSON object with "row" and "col" fields (0-2).
Example: {{"row": 1, "col": 1}}

Think step by step and make the best strategic move.
"""
        
        try:
            response = requests.post(
                f"{self.agent1_url}/v1/chat/completions",
                json={
                    "model": "gemma-3-270m-it-qat",
                    "messages": [{"role": "user", "content": prompt}],
                    "temperature": 0.7
                },
                timeout=30
            )
            
            if response.status_code == 200:
                content = response.json()["choices"][0]["message"]["content"]
                # Try to extract JSON from the response
                import re
                json_match = re.search(r'\\{.*\\}', content)
                if json_match:
                    move = json.loads(json_match.group())
                    row = int(move.get("row")) if "row" in move else None
                    col = int(move.get("col")) if "col" in move else None
                    if self.is_valid_move_candidate(row, col):
                        return row, col
        except Exception as e:
            print(f"Error getting move from agent 1 (LM Studio): {e}")
        
        # Default move if there's an error
        return self.get_default_move()
    
    def get_move_from_agent2(self, game_state):
        """Get a move from the Ollama AI agent"""
        prompt = f"""
You are playing TicTacToe as 'O'. Analyze the current board state and make a strategic move.

Current board state:
{self.format_board_for_prompt()}

Analyze the board carefully:
1. Look for any possible winning moves for you (O)
2. Look for any moves your opponent (X) needs to be blocked
3. Consider strategic positions (center, corners)
4. Think about your next move strategically

Provide your move as a JSON object with "row" and "col" fields (0-2).
Example: {{"row": 1, "col": 1}}

Think step by step and make the best strategic move.
"""
        
        try:
            # For Ollama, we make a REST API call to port 11434
            response = requests.post(
                f"{self.agent2_url}/api/generate",
                json={
                    "model": "gemma3:270m",
                    "prompt": prompt,
                    "stream": False
                },
                timeout=30
            )
            
            if response.status_code == 200:
                content = response.json()["response"]
                # Try to extract JSON from the response
                import re
                json_match = re.search(r'\\{.*\\}', content)
                if json_match:
                    move = json.loads(json_match.group())
                    row = int(move.get("row")) if "row" in move else None
                    col = int(move.get("col")) if "col" in move else None
                    if self.is_valid_move_candidate(row, col):
                        return row, col
        except Exception as e:
            print(f"Error getting move from agent 2 (Ollama): {e}")
        
        # Default move if there's an error
        return self.get_default_move()
    
    def format_board_for_prompt(self):
        """Format the board for the AI prompt with better visual representation"""
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
    
    def get_strategic_analysis(self):
        """Provide strategic analysis for the AI agents"""
        analysis = []
        
        # Check for winning moves
        for i in range(3):
            for j in range(3):
                if self.game_board[i][j] is None:
                    # Simulate placing X or O here
                    # This is a simplified analysis - in a real implementation,
                    # we would do a more thorough check
                    pass
        
        if len(analysis) == 0:
            analysis.append("No immediate threats detected. Consider controlling the center or a corner.")
            
        return "\n".join(analysis)
    
    def get_default_move(self):
        """Get a default move by finding available empty cells and selecting randomly"""
        import random
        
        # Find all empty cells
        empty_cells = []
        for i in range(3):
            for j in range(3):
                if self.game_board[i][j] is None:
                    empty_cells.append((i, j))
        
        # If no empty cells, return default
        if not empty_cells:
            return 0, 0
            
        # Prioritize center, then corners, then edges
        center = [(1, 1)]
        corners = [(0, 0), (0, 2), (2, 0), (2, 2)]
        edges = [(0, 1), (1, 0), (1, 2), (2, 1)]
        
        # Filter available moves by priority
        available_center = [cell for cell in center if cell in empty_cells]
        available_corners = [cell for cell in corners if cell in empty_cells]
        available_edges = [cell for cell in edges if cell in empty_cells]
        
        # Select from highest priority available group
        if available_center:
            return random.choice(available_center)
        elif available_corners:
            return random.choice(available_corners)
        elif available_edges:
            return random.choice(available_edges)
        else:
            # Fallback to random selection from all empty cells
            return random.choice(empty_cells)

    def is_valid_move_candidate(self, row, col):
        """Quick validation for an agent-provided move (bounds and empty cell)."""
        try:
            if row is None or col is None:
                return False
            if not (0 <= int(row) <= 2 and 0 <= int(col) <= 2):
                return False
            return self.game_board[int(row)][int(col)] is None
        except Exception:
            return False
    
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
    
    def update_board(self, row, col, player):
        """Update the board with the player's move"""
        self.game_board[row][col] = player
    
    def start_game(self, agent1_pubkey, agent2_pubkey):
        """Start a new TicTacToe game"""
        # Log which addresses/pubkeys we're using to start the on-chain episode
        self.log('INFO', f'Starting episode with participants: {agent1_pubkey}, {agent2_pubkey}')

        # If we require on-chain txs, ensure signer/node are configured
        if self.require_onchain and not self.has_onchain_capability():
            self.log('ERROR', 'On-chain mode enabled but SIGNER_ADDRESS or NODE_RPC_URL is not configured. Refusing to start episode.')
            return False

        # Prefer server-side agent pubkeys if available to ensure authorization works
        p1 = self.agent1_pubkey or agent1_pubkey
        p2 = self.agent2_pubkey or agent2_pubkey

        response = self.send_mcp_request("tools/call", {
            "name": "kdapp_start_episode",
            "arguments": {
                "participants": [p1, p2]
            }
        })

        if "result" in response:
            self.episode_id = response["result"]
            self.log('INFO', f'Game started with episode ID: {self.episode_id}')

            # Immediately query episode state to show participants and on-chain metadata
            state = self.get_game_state()
            if state and isinstance(state, dict):
                participants = state.get('participants') or state.get('metadata', {}).get('participants')
                if participants:
                    self.log('INFO', f'Episode participants (onchain): {participants}')
                else:
                    self.log('DEBUG', f'kdapp_get_episode_state returned: {state}')

            return True
        else:
            self.log('ERROR', f'Error starting game: {response}')
            return False

    def send_mcp_request(self, method, params=None):
        """Send a request to the kdapp MCP server"""
        request = {
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params or {}
        }
        # Send request to server
        try:
            self.log('DEBUG', f'Sending MCP request: {method} {params or {}}')
        except Exception:
            pass
        self.server_process.stdin.write(json.dumps(request) + "\n")
        self.server_process.stdin.flush()

        # Read response: skip non-JSON lines (cargo/build logs) and parse the first JSON
        start = time.time()
        while True:
            if time.time() - start > 5:
                raise TimeoutError("Timed out waiting for MCP server JSON response")
            response_line = self.server_process.stdout.readline()
            if not response_line:
                # short sleep to avoid busy loop
                time.sleep(0.01)
                continue
            line = response_line.strip()
            try:
                parsed = json.loads(line)
                try:
                    self.log('DEBUG', f'Received MCP response: {parsed}')
                except Exception:
                    pass
                return parsed
            except Exception:
                # not JSON, keep reading
                continue
    
    def play_game(self):
        """Main game loop"""
        # Start the game using generated agent addresses
        if not self.start_game(self.agent1_addr, self.agent2_addr):
            print("Failed to start game")
            return

        # Game loop
        move_count = 0
        max_moves = 9
        consecutive_failures = 0
        max_consecutive_failures = 3
        last_move = None
        
        while move_count < max_moves and consecutive_failures < max_consecutive_failures:
            self.print_board()

            # Check for winner or draw
            winner = self.check_winner()
            if winner:
                if winner == "Draw":
                    print("Game ended in a draw!")
                else:
                    print(f"Player {winner} wins!")
                break

            # Get move from current player
            if self.current_player == "X":
                # Teal for Agent 1
                print("\033[38;5;37mAgent 1's turn (X)\033[0m")
                signer = "agent1"
                row, col = self.get_move_from_agent1(self.game_board)
            else:
                # Orange for Agent 2
                print("\033[38;5;208mAgent 2's turn (O)\033[0m")
                signer = "agent2"
                row, col = self.get_move_from_agent2(self.game_board)

            # Execute the move
            if self.execute_move(self.current_player, row, col, signer):
                color = "\033[38;5;37m" if self.current_player == "X" else "\033[38;5;208m"
                print(f"{color}Player {self.current_player} moves to ({row}, {col})\033[0m")
                self.update_board(row, col, self.current_player)

                # Switch players
                self.current_player = "O" if self.current_player == "X" else "X"
                move_count += 1
                consecutive_failures = 0  # Reset failure counter on success
                last_move = (row, col)  # Track last move
            else:
                print("Invalid move, trying again...")
                consecutive_failures += 1
                last_move = (row, col)  # Track last failed move
                
                # Try a default move as fallback
                default_row, default_col = self.get_default_move()
                # Make sure we don't try the same invalid move again
                if default_row == row and default_col == col:
                    # Find another empty cell that's different from the last move
                    found_alternative = False
                    for i in range(3):
                        for j in range(3):
                            if self.game_board[i][j] is None and not (i == row and j == col):
                                default_row, default_col = i, j
                                found_alternative = True
                                break
                        if found_alternative:
                            break
                
                if self.execute_move(self.current_player, default_row, default_col, signer):
                    color = "\033[38;5;37m" if self.current_player == "X" else "\033[38;5;208m"
                    print(f"{color}Player {self.current_player} moves to ({default_row}, {default_col}) using default move\033[0m")
                    self.update_board(default_row, default_col, self.current_player)
                    self.current_player = "O" if self.current_player == "X" else "X"
                    move_count += 1
                    consecutive_failures = 0  # Reset failure counter on success
                    last_move = (default_row, default_col)  # Track last move
                else:
                    print("Failed to execute even default move")
                    last_move = (default_row, default_col)  # Track last failed move

        # Final board state
        self.print_board()
        
        # Check final winner if game ended due to move limit or board full
        winner = self.check_winner()
        if winner == "Draw":
            print("Game ended in a draw!")
        elif winner:
            print(f"Player {winner} wins!")
        else:
            print("Game ended with no winner.")

if __name__ == "__main__":
    coordinator = TicTacToeCoordinator()
    coordinator.play_game()
