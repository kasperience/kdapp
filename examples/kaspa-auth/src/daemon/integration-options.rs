// Integration Options for WebSocket Authentication

// OPTION 1: Add WebSocket endpoint to existing HTTP server
// src/api/http/organizer_peer.rs - Add WebSocket support
pub async fn run_http_peer_with_websocket(
    provided_private_key: Option<&str>, 
    port: u16
) -> Result<(), Box<dyn std::error::Error>> {
    // ... existing setup ...
    
    let app = Router::new()
        // Existing HTTP routes
        .route("/health", get(health))
        .route("/auth/start", post(start_auth))
        .route("/auth/request-challenge", post(request_challenge))
        .route("/auth/verify", post(verify_auth))
        .route("/auth/status/{episode_id}", get(get_status))
        
        // ADD: WebSocket endpoint
        .route("/ws-auth", get(websocket_auth_handler))
        
        // Existing static file serving
        .fallback_service(ServeDir::new("public"))
        .with_state(peer_state)
        .layer(cors);
    
    // ... rest of server setup ...
}

// OPTION 2: Run separate WebSocket server alongside HTTP
// src/main.rs - Add WebSocket mode
impl Commands {
    pub async fn execute(self, keychain: bool, dev_mode: bool) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            // ... existing commands ...
            
            // ADD: New WebSocket server command
            Commands::WebSocketPeer(cmd) => {
                let wallet = get_wallet_for_command("websocket-peer", cmd.key.as_deref())?;
                crate::api::websocket::start_ws_auth_server(wallet.keypair, cmd.port).await
            },
            
            // ... rest of commands ...
        }
    }
}

// OPTION 3: Hybrid approach - Add WebSocket to daemon
// src/daemon/service.rs - Add WebSocket support to daemon
impl AuthDaemon {
    pub async fn run_with_websocket(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Start IPC socket listener (existing)
        let ipc_task = self.run_ipc_listener();
        
        // ADD: Start WebSocket server for web clients
        let ws_task = self.run_websocket_server();
        
        tokio::select! {
            result = ipc_task => result,
            result = ws_task => result,
        }
    }
    
    async fn run_websocket_server(&self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = "127.0.0.1:8082"; // Daemon WebSocket port
        let listener = tokio::net::TcpListener::bind(addr).await?;
        
        println!("🌐 Daemon WebSocket server listening on ws://{}", addr);
        
        while let Ok((stream, _)) = listener.accept().await {
            let daemon = self.clone();
            tokio::spawn(async move {
                if let Ok(ws) = tokio_tungstenite::accept_async(stream).await {
                    daemon.handle_websocket_client(ws).await;
                }
            });
        }
        
        Ok(())
    }
}