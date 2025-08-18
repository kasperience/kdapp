# Local AI Agents Guide (LM Studio + Ollama)

This example coordinates two local AI agents to play TicTacToe via the kdapp MCP server. You can run both agents entirely on your machine with small models that fit modest hardware.

Quick Summary
- Agent 1 (X): LM Studio server on `http://127.0.0.1:1234` using an instruct chat model.
- Agent 2 (O): Ollama server on `http://127.0.0.1:11434` using `gemma3:270m` (very small, fast to run locally).

Requirements
- Windows/macOS/Linux supported.
- For WSL users: agents run on Windows, MCP server in WSL is fine (see WSL notes below).

Install LM Studio (Agent 1)
1. Download: https://lmstudio.ai/
2. Launch LM Studio and download a small instruct chat model, e.g.:
   - “Gemma 2 2B Instruct” or “Phi-3 Mini” or another small instruct model available in LM Studio.
3. Start the local server:
   - Settings → Developer → Enable Local Server.
   - Default REST endpoint: `http://127.0.0.1:1234/v1` (OpenAI-compatible).
4. Test with curl:
   ```bash
   curl http://127.0.0.1:1234/v1/models
   ```

Install Ollama (Agent 2)
1. Install: https://ollama.com/
2. Pull a very small model:
   ```bash
   ollama pull gemma3:270m
   ```
3. Run the model (starts/uses the local API on port 11434):
   ```bash
   ollama run gemma3:270m
   ```
   You can keep this terminal open; the kdapp coordinator will call the REST API at `http://127.0.0.1:11434`.

Endpoints Expected by the Coordinator
- LM Studio (Agent 1): `POST http://<host>:1234/v1/chat/completions`
  - OpenAI-style JSON body with `model` and `messages`.
- Ollama (Agent 2): `POST http://<host>:11434/api/generate`
  - JSON body: `{ "model": "gemma3:270m", "prompt": "...", "stream": false }`.

WSL/Windows Notes
- If kdapp server runs in WSL and agents on Windows, coordinator will try to detect the Windows host IP via `/etc/resolv.conf`.
- If detection fails, set host to `127.0.0.1` or replace with your Windows host IP.

Troubleshooting
- Connection refused:
  - Ensure LM Studio server is enabled and a model is loaded.
  - Ensure Ollama is running and the model is pulled.
  - Check firewall for ports 1234 and 11434.
- CORS (LM Studio): enable CORS in LM Studio settings if needed.
- Bind address: prefer `127.0.0.1` or ensure server binds to `0.0.0.0` if accessed across WSL/host boundary.

Minimal Test Calls
- LM Studio chat completion:
  ```bash
  curl -s http://127.0.0.1:1234/v1/chat/completions \
    -H 'Content-Type: application/json' \
    -d '{
          "model": "<your-lm-studio-model-name>",
          "messages": [{"role": "user", "content": "Say hello"}],
          "temperature": 0.7
        }'
  ```
- Ollama generate:
  ```bash
  curl -s http://127.0.0.1:11434/api/generate \
    -H 'Content-Type: application/json' \
    -d '{
          "model": "gemma3:270m",
          "prompt": "Say hello",
          "stream": false
        }'
  ```

Coordinator Defaults
- Agent 1 uses LM Studio at port 1234; Agent 2 uses Ollama at port 11434.
- The script extracts `{row, col}` from the model’s response. If parsing fails, it falls back to a safe default move.

