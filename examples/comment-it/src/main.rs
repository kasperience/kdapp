use clap::Parser;
use comment_it::episode_runner::create_auth_generator;
use comment_it::wallet;
use futures_util::sink::SinkExt;
use futures_util::stream::StreamExt;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::tx::Transaction;
use kaspa_wrpc_client::prelude::RpcApi;
use kdapp::engine::EpisodeMessage;
use kdapp::pki::PubKey;
use kdapp::proxy::{connect_client, connect_options};
use log::{info, warn};
use secp256k1::Keypair;
use serde::{Deserialize, Serialize};
use std::error::Error;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::accept_async;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
enum Cli {
    /// Legacy pure-kdapp WebSocket peer (kept for dev)
    #[command(name = "ws-peer")]
    WsPeer {
        /// WebSocket server address to listen on
        #[arg(long, default_value = "127.0.0.1:8080")]
        ws_addr: String,
    },
    /// HTTP coordination peer (organizer) serving UI and API
    #[command(name = "http-peer")]
    HttpPeer {
        /// Port for HTTP server
        #[arg(long, default_value = "8080")]
        port: u16,
        /// Optional private key (hex) for signer wallet
        #[arg(long)]
        key: Option<String>,
        /// Optional Kaspa wRPC URL
        #[arg(long, value_name = "URL")]
        wrpc_url: Option<String>,
        /// Retry count for RPC submits
        #[arg(long, value_name = "N", default_value_t = 3)]
        rpc_retry: usize,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    // Get a persistent wallet for the peer
    let signer_wallet = wallet::get_wallet_for_command("comment-it-peer", None)?;
    let signer_keypair = signer_wallet.keypair;
    let signer_address = signer_wallet.get_kaspa_address();

    // WS peer builds transactions directly; HTTP peer sets up its own engine.

    match cli {
        Cli::HttpPeer { port, key, wrpc_url: _wrpc_url, rpc_retry: _rpc_retry } => {
            // Dispatch directly to HTTP organizer peer server
            info!("👂 HTTP organizer peer listening on: 0.0.0.0:{port}");
            comment_it::api::http::organizer_peer::run_http_peer(key.as_deref(), port).await?;
            return Ok(());
        }
        Cli::WsPeer { ws_addr } => {
            // Start WebSocket server for frontend communication
            let listener = TcpListener::bind(&ws_addr).await?;
            info!("👂 WebSocket server listening on: {ws_addr}");

            // Initialize a shared kaspad client once and reuse (clone) per connection
            let network_id = NetworkId::with_suffix(NetworkType::Testnet, 10);
            let shared_kaspad = connect_client(network_id, None).await?;

            while let Ok((stream, _)) = listener.accept().await {
                let kaspad_client = shared_kaspad.clone();
                tokio::spawn(handle_connection(stream, signer_keypair, signer_address.clone(), kaspad_client, network_id));
            }
        } // close match cli
    }

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum FrontendCommand {
    SubmitComment { text: String, episode_id: u64 },
    RequestChallenge { episode_id: u64 },
    SubmitResponse { episode_id: u64, signature: String, nonce: String },
    RevokeSession { episode_id: u64, signature: String },
}

async fn handle_connection(
    stream: TcpStream,
    signer_keypair: Keypair,
    signer_address: kaspa_addresses::Address,
    kaspad_client: kaspa_wrpc_client::KaspaRpcClient,
    network_id: NetworkId,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ws_stream = accept_async(stream).await?;
    info!("✅ New WebSocket connection established");

    let (mut write, mut read) = ws_stream.split();

    // No event forwarding in this variant; WS remains request/response only

    while let Some(message) = read.next().await {
        let message = message?;
        if message.is_text() {
            let text = message.to_text()?;
            info!("Received message from frontend: {text}");

            match serde_json::from_str::<FrontendCommand>(text) {
                Ok(FrontendCommand::SubmitComment { text, episode_id }) => {
                    info!("Processing SubmitComment command for episode {episode_id}: {text}");

                    let command = comment_it::core::UnifiedCommand::SubmitComment { text: text.clone(), session_token: String::new() };
                    let public_key = PubKey(signer_keypair.public_key());
                    let episode_message = EpisodeMessage::<comment_it::core::AuthWithCommentsEpisode>::new_signed_command(
                        episode_id.try_into().unwrap(),
                        command,
                        signer_keypair.secret_key(),
                        public_key,
                    );

                    // Connect to kaspad for UTXO fetching and transaction submission
                    // Fetch UTXOs for the signer's address
                    let entries = kaspad_client.get_utxos_by_addresses(vec![signer_address.clone()]).await?;
                    let first = entries.first().ok_or("No UTXOs found. Wallet needs funding.")?;
                    let utxo = (
                        kaspa_consensus_core::tx::TransactionOutpoint::from(first.outpoint),
                        kaspa_consensus_core::tx::UtxoEntry::from(first.utxo_entry.clone()),
                    );

                    let tx_generator = create_auth_generator(signer_keypair, network_id);
                    let tx = tx_generator.build_command_transaction(utxo, &signer_address.clone(), &episode_message, 1000); // 1000 is placeholder fee

                    match submit_tx_retry(&kaspad_client, tx.as_ref(), 3).await {
                        Ok(()) => {
                            let tx_id = tx.id().to_string();
                            info!("✅ Transaction submitted: {tx_id}");
                            let response = serde_json::to_string(&serde_json::json!({ "status": "submitted", "tx_id": tx_id }))?;
                            write.send(tokio_tungstenite::tungstenite::Message::text(response)).await?;
                        }
                        Err(e) => {
                            warn!("❌ Failed to submit transaction: {e}");
                            let error_response = serde_json::to_string(
                                &serde_json::json!({ "status": "error", "message": format!("Failed to submit transaction: {}", e) }),
                            )?;
                            write.send(tokio_tungstenite::tungstenite::Message::text(error_response)).await?;
                        }
                    }
                }
                Ok(FrontendCommand::RequestChallenge { episode_id }) => {
                    info!("Processing RequestChallenge command for episode {episode_id}");
                    let command = comment_it::core::UnifiedCommand::RequestChallenge;
                    let public_key = PubKey(signer_keypair.public_key());
                    let episode_message = EpisodeMessage::<comment_it::core::AuthWithCommentsEpisode>::new_signed_command(
                        episode_id.try_into().unwrap(),
                        command,
                        signer_keypair.secret_key(),
                        public_key,
                    );

                    let entries = kaspad_client.get_utxos_by_addresses(vec![signer_address.clone()]).await?;
                    let first = entries.first().ok_or("No UTXOs found. Wallet needs funding.")?;
                    let utxo = (
                        kaspa_consensus_core::tx::TransactionOutpoint::from(first.outpoint),
                        kaspa_consensus_core::tx::UtxoEntry::from(first.utxo_entry.clone()),
                    );

                    let tx_generator = create_auth_generator(signer_keypair, network_id);
                    let tx = tx_generator.build_command_transaction(utxo, &signer_address.clone(), &episode_message, 1000);

                    match submit_tx_retry(&kaspad_client, tx.as_ref(), 3).await {
                        Ok(()) => {
                            let tx_id = tx.id().to_string();
                            info!("✅ RequestChallenge transaction submitted: {tx_id}");
                            let response = serde_json::to_string(
                                &serde_json::json!({ "status": "submitted", "tx_id": tx_id, "command": "RequestChallenge" }),
                            )?;
                            write.send(tokio_tungstenite::tungstenite::Message::text(response)).await?;
                        }
                        Err(e) => {
                            warn!("❌ Failed to submit RequestChallenge transaction: {e}");
                            let error_response = serde_json::to_string(
                                &serde_json::json!({ "status": "error", "message": format!("Failed to submit RequestChallenge: {}", e) }),
                            )?;
                            write.send(tokio_tungstenite::tungstenite::Message::text(error_response)).await?;
                        }
                    }
                }
                Ok(FrontendCommand::SubmitResponse { episode_id, signature, nonce }) => {
                    info!("Processing SubmitResponse command for episode {episode_id}");
                    let command = comment_it::core::UnifiedCommand::SubmitResponse { signature, nonce };
                    let public_key = PubKey(signer_keypair.public_key());
                    let episode_message = EpisodeMessage::<comment_it::core::AuthWithCommentsEpisode>::new_signed_command(
                        episode_id.try_into().unwrap(),
                        command,
                        signer_keypair.secret_key(),
                        public_key,
                    );

                    let entries = kaspad_client.get_utxos_by_addresses(vec![signer_address.clone()]).await?;
                    let first = entries.first().ok_or("No UTXOs found. Wallet needs funding.")?;
                    let utxo = (
                        kaspa_consensus_core::tx::TransactionOutpoint::from(first.outpoint),
                        kaspa_consensus_core::tx::UtxoEntry::from(first.utxo_entry.clone()),
                    );

                    let tx_generator = create_auth_generator(signer_keypair, network_id);
                    let tx = tx_generator.build_command_transaction(utxo, &signer_address.clone(), &episode_message, 1000);

                    match submit_tx_retry(&kaspad_client, tx.as_ref(), 3).await {
                        Ok(()) => {
                            let tx_id = tx.id().to_string();
                            info!("✅ SubmitResponse transaction submitted: {tx_id}");
                            let response = serde_json::to_string(
                                &serde_json::json!({ "status": "submitted", "tx_id": tx_id, "command": "SubmitResponse" }),
                            )?;
                            write.send(tokio_tungstenite::tungstenite::Message::text(response)).await?;
                        }
                        Err(e) => {
                            warn!("❌ Failed to submit SubmitResponse transaction: {e}");
                            let error_response = serde_json::to_string(
                                &serde_json::json!({ "status": "error", "message": format!("Failed to submit SubmitResponse: {}", e) }),
                            )?;
                            write.send(tokio_tungstenite::tungstenite::Message::text(error_response)).await?;
                        }
                    }
                }
                Ok(FrontendCommand::RevokeSession { episode_id, signature }) => {
                    info!("Processing RevokeSession command for episode {episode_id}");
                    let command = comment_it::core::UnifiedCommand::RevokeSession { session_token: String::new(), signature };
                    let public_key = PubKey(signer_keypair.public_key());
                    let episode_message = EpisodeMessage::<comment_it::core::AuthWithCommentsEpisode>::new_signed_command(
                        episode_id.try_into().unwrap(),
                        command,
                        signer_keypair.secret_key(),
                        public_key,
                    );

                    let entries = kaspad_client.get_utxos_by_addresses(vec![signer_address.clone()]).await?;
                    let first = entries.first().ok_or("No UTXOs found. Wallet needs funding.")?;
                    let utxo = (
                        kaspa_consensus_core::tx::TransactionOutpoint::from(first.outpoint),
                        kaspa_consensus_core::tx::UtxoEntry::from(first.utxo_entry.clone()),
                    );

                    let tx_generator = create_auth_generator(signer_keypair, network_id);
                    let tx = tx_generator.build_command_transaction(utxo, &signer_address.clone(), &episode_message, 1000);

                    match submit_tx_retry(&kaspad_client, tx.as_ref(), 3).await {
                        Ok(()) => {
                            let tx_id = tx.id().to_string();
                            info!("✅ RevokeSession transaction submitted: {tx_id}");
                            let response = serde_json::to_string(
                                &serde_json::json!({ "status": "submitted", "tx_id": tx_id, "command": "RevokeSession" }),
                            )?;
                            write.send(tokio_tungstenite::tungstenite::Message::text(response)).await?;
                        }
                        Err(e) => {
                            warn!("❌ Failed to submit RevokeSession transaction: {e}");
                            let error_response = serde_json::to_string(
                                &serde_json::json!({ "status": "error", "message": format!("Failed to submit RevokeSession: {}", e) }),
                            )?;
                            write.send(tokio_tungstenite::tungstenite::Message::text(error_response)).await?;
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to parse frontend command: {text}. Error: {e}");
                    let error_response = serde_json::to_string(
                        &serde_json::json!({ "status": "error", "message": format!("Invalid command: {}", e) }),
                    )?;
                    write.send(tokio_tungstenite::tungstenite::Message::text(error_response)).await?;
                }
            }
        } else if message.is_binary() {
            warn!("Received binary message, not supported yet.");
        }
    }

    info!("❌ WebSocket connection closed");
    Ok(())
}

async fn submit_tx_retry(kaspad: &kaspa_wrpc_client::KaspaRpcClient, tx: &Transaction, attempts: usize) -> Result<(), String> {
    let mut tries = 0usize;
    loop {
        match kaspad.submit_transaction(tx.into(), false).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                tries += 1;
                let msg = e.to_string();
                if msg.contains("already accepted") {
                    return Ok(());
                }
                if tries >= attempts {
                    return Err(msg.to_string());
                }
                if msg.contains("WebSocket") || msg.contains("not connected") || msg.contains("disconnected") {
                    let _ = kaspad.connect(Some(connect_options())).await;
                    continue;
                }
                if msg.to_lowercase().contains("orphan") {
                    // brief retry for orphan case
                    continue;
                }
                return Err(msg.to_string());
            }
        }
    }
}
