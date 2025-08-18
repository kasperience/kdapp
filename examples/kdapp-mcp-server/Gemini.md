# Gemini's Roadmap: Rust MCP Server for kdapp

## 1. Vision & Goal

The primary goal is to build a high-performance, pure Rust MCP (Model Context Protocol) server. This server will directly leverage the existing `kdapp` Rust modules (`Engine`, `Proxy`, `Episode`, `TxGenerator`).

By doing this, we create a native Rust application that can be used by any MCP-compliant AI agent. The agent will interact with high-level tools that correspond to the `kdapp` architecture, providing a robust and powerful interface.

The server will operate by reading JSON-RPC 2.0 requests from `stdin` and writing JSON-RPC 2.0 responses to `stdout`.

## 2. Phased Implementation Plan

### Phase 1: Project Scaffolding & Core Dependencies

- **1.1. Clean Slate:** Remove the existing TypeScript-specific files (`.ts`, `tsconfig.json`, `package.json`, etc.) to create a clean project directory.
- **1.2. `Cargo.toml`:** Create the project manifest. This will define the new crate and its dependencies.
- **1.3. Dependencies:** Add essential crates:
    - `tokio`: For the asynchronous runtime (handling `stdin`/`stdout`).
    - `serde` & `serde_json`: For serializing and deserializing the JSON-RPC messages.
    - `log` & `env_logger`: For robust logging.
    - `anyhow`: For flexible error handling.
- **1.4. Project Structure:** Establish a standard Rust project structure with `src/main.rs` as the entry point.

### Phase 2: MCP Communication Layer

- **2.1. Async Main:** Set up an asynchronous `main` function in `src/main.rs` using `tokio`.
- **2.2. I/O Loop:** Implement the core server loop to read lines asynchronously from `stdin`.
- **2.3. Message Parsing:** For each incoming line, parse it into a strongly-typed JSON-RPC `Request` struct.
- **2.4. Request Dispatcher:** Create a central function that takes a request and routes it to the appropriate handler based on the `method` field (e.g., `tools/list`, `tools/call`).
- **2.5. Response Handling:** Implement logic to serialize the `Response` structs (both success and error) back into JSON and write them to `stdout`.

### Phase 3: `kdapp` Integration & Logic

- **3.1. Module Integration:** Integrate the existing `kdapp` source files (`engine.rs`, `episode.rs`, etc.) as modules within the new crate.
- **3.2. Server State:** Create a `ServerState` struct that holds the `kdapp::engine::Engine` and any other shared state.
- **3.3. Tool Definition:** Define the new high-level MCP tools that will be exposed to the AI agent. This will include:
    - `kdapp_start_episode(participants: Vec<PubKey>): EpisodeId`
    - `kdapp_execute_command(episode_id: EpisodeId, command: Command, signature?: Sig)`
    - `kdapp_get_episode_state(episode_id: EpisodeId): any`
    - `kdapp_generate_transaction(command: EpisodeMessage): Transaction`
- **3.4. Tool Implementation:** Write the Rust functions that implement the logic for each tool. These functions will call into the `kdapp::engine` and other modules to perform their tasks.

### Phase 4: Finalization & Verification

- **4.1. Compilation & Build:** Regularly compile the project using `cargo check` and `cargo build` to ensure everything is correct.
- **4.2. Run:** Use `cargo run` to launch the server and prepare it for interaction.
- **4.3. Cleanup:** Once the Rust server is functional, we can remove the now-obsolete `kdapp` subdirectory that was copied for reference.

