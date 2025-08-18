import json
import subprocess
import sys
import time

class SimpleTester:
    def __init__(self):
        # Start the kdapp MCP server as a subprocess
        self.server_process = subprocess.Popen(
            ['cargo', 'run', '--bin', 'kdapp-mcp-server'],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,  # merge stderr for robust startup detection
            text=True,
            encoding='utf-8',
            bufsize=1
        )

        # Wait for the server to be ready before sending requests
        print('Waiting for kdapp MCP server to start...')
        start_time = time.time()
        ready = False
        readiness_markers = (
            'connected to kaspa node successfully',
            'successfully connected to kaspa node',
            'continues in offline mode',
            'server continues in offline mode',
            'finished dev',
            'running `target',
        )
        while time.time() - start_time < 90:  # allow first build
            line = self.server_process.stdout.readline()
            if line:
                stripped = line.strip()
                print(f"[Server Startup]: {stripped}", file=sys.stderr)
                if any(m in stripped.lower() for m in readiness_markers):
                    print('Server is ready.')
                    ready = True
                    break
            else:
                time.sleep(0.05)

        if not ready:
            print('Timeout waiting for server to start.')
            # Let's see what we got from the server
            print("Checking remaining server output...")
            try:
                remaining = self.server_process.stdout.read() or ''
                if remaining:
                    print(f"[Server Startup - tail]:\n{remaining}", file=sys.stderr)
            except Exception:
                pass
                
            raise RuntimeError("kdapp MCP server failed to start in time.")

    def send_mcp_request(self, method, params=None):
        request = {
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params or {}
        }
        print(f'--> Sending: {json.dumps(request)}')
        self.server_process.stdin.write(json.dumps(request) + "\n")
        self.server_process.stdin.flush()

        start = time.time()
        while True:
            if time.time() - start > 10:
                raise TimeoutError("Timed out waiting for MCP server JSON response")
            response_line = self.server_process.stdout.readline()
            if not response_line:
                time.sleep(0.01)
                continue
            line = response_line.strip()
            try:
                parsed = json.loads(line)
                print(f'<-- Received: {json.dumps(parsed)}')
                return parsed
            except Exception:
                print(f"[Server stdout]: {line}", file=sys.stderr)
                continue

    def run_test(self):
        try:
            # 1. List tools to see what's available
            print("\n--- Listing Tools ---")
            tools_response = self.send_mcp_request("tools/list")
            print(f"Available tools: {tools_response}")

            # Fetch server agent pubkeys for authorized episode
            print("\n--- Fetching Agent Pubkeys ---")
            pk_resp = self.send_mcp_request("tools/call", {"name": "kdapp_get_agent_pubkeys"})
            a1 = pk_resp.get('result', {}).get('agent1_pubkey')
            a2 = pk_resp.get('result', {}).get('agent2_pubkey')

            # 2. Start an episode with real agent public keys
            print("\n--- Starting Episode ---")
            # Use server wallet pubkeys for correct authorization
            start_response = self.send_mcp_request("tools/call", {
                "name": "kdapp_start_episode",
                "arguments": {
                    "participants": [a1, a2]
                }
            })
            episode_id = start_response['result']
            print(f"Episode started with ID: {episode_id}")

            # 3. Execute a move for Agent 1
            print("\n--- Agent 1 Move ---")
            move1_response = self.send_mcp_request("tools/call", {
                "name": "kdapp_execute_command",
                "arguments": {
                    "episode_id": episode_id,
                    "command": {"type": "move", "player": "X", "row": 0, "col": 0},
                    "signer": "agent1"
                }
            })
            print(f"Agent 1 move response: {move1_response}")

            # 4. Execute a move for Agent 2
            print("\n--- Agent 2 Move ---")
            move2_response = self.send_mcp_request("tools/call", {
                "name": "kdapp_execute_command",
                "arguments": {
                    "episode_id": episode_id,
                    "command": {"type": "move", "player": "O", "row": 1, "col": 1},
                    "signer": "agent2"
                }
            })
            print(f"Agent 2 move response: {move2_response}")

        finally:
            print("\n--- Terminating Server ---")
            self.server_process.terminate()

if __name__ == "__main__":
    tester = SimpleTester()
    tester.run_test()
