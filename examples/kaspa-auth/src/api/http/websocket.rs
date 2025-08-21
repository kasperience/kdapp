// src/api/http/websocket.rs
use crate::api::http::state::PeerState;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::Response,
};
use log::info;
use tokio::select;

pub async fn websocket_handler(ws: WebSocketUpgrade, State(state): State<PeerState>) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: PeerState) {
    info!("New WebSocket connection established");

    // Subscribe to broadcast messages
    let mut rx = state.websocket_tx.subscribe();

    loop {
        select! {
            // Listen for broadcast messages from the organizer peer
            msg = rx.recv() => {
                match msg {
                    Ok(ws_message) => {
                        let json_str = match serde_json::to_string(&ws_message) {
                            Ok(json) => json,
                            Err(e) => {
                                eprintln!("Failed to serialize WebSocket message: {}", e);
                                continue;
                            }
                        };

                        if socket.send(Message::Text(json_str.into())).await.is_err() {
                            info!("WebSocket connection closed");
                            break;
                        }
                    }
                    Err(_) => {
                        // Channel closed
                        break;
                    }
                }
            }

            // Listen for incoming messages from participant peer (optional)
            socket_msg = socket.recv() => {
                match socket_msg {
                    Some(Ok(_)) => {
                        // Handle participant peer messages if needed
                        // For now, just continue
                    }
                    _ => {
                        info!("WebSocket connection closed by participant peer");
                        break;
                    }
                }
            }
        }
    }
}
