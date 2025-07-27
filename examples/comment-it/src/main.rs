use std::error::Error;
use log::{info, warn, error};
use comment_it::episode_runner::{AuthServerConfig, run_auth_server, create_auth_generator};
use comment_it::wallet;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::accept_async;
use futures_util::stream::StreamExt;
use futures_util::sink::SinkExt;
use serde::{Serialize, Deserialize};
use kdapp::episode::EpisodeMessage;
use kdapp::pki::PubKey;
use kdapp::generator::TransactionGenerator;
use kdapp::proxy::connect_client;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use secp256k1::Keypair;
use tokio::sync::mpsc;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// WebSocket server address to listen on
    #[arg(long, default_value = "127.0.0.1:8080")]
    ws_addr: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    info!("🚀 Starting pure kdapp comment-it peer...");

    // Get a persistent wallet for the peer
    let signer_wallet = wallet::get_wallet_for_command("comment-it-peer", None)?;
    let signer_keypair = signer_wallet.keypair;
    let signer_address = signer_wallet.kaspa_address;

    // Create a transaction generator
    let tx_generator = create_auth_generator(signer_keypair.clone(), NetworkId::with_suffix(NetworkType::Testnet, 10));

    // Create a channel for sending events from AuthEventHandler to WebSocket connections
    let (event_tx, event_rx) = mpsc::channel(100); // Buffer size of 100 events

    // Start the kdapp engine and listener
    let auth_config = AuthServerConfig::new(
        signer_keypair.clone(),
        "comment-it-peer".to_string(),
        None, // No specific RPC URL, use default
    );
    tokio::spawn(async move { run_auth_server(auth_config, event_tx).await });

    // Start WebSocket server for frontend communication
    let listener = TcpListener::bind(&cli.ws_addr).await?;
    info!("👂 WebSocket server listening on: {}", cli.ws_addr);

    // Clone the event receiver for each new connection
    let event_rx_arc = std::sync::Arc::new(tokio::sync::Mutex::new(event_rx));

    while let Ok((stream, _)) = listener.accept().await {
        let peer_event_rx = event_rx_arc.clone();
        tokio::spawn(handle_connection(stream, signer_keypair.clone(), tx_generator.clone(), signer_address.clone(), peer_event_rx));
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

async fn handle_connection(stream: TcpStream, signer_keypair: Keypair, tx_generator: TransactionGenerator, signer_address: String, mut event_rx: std::sync::Arc<tokio::sync::Mutex<mpsc::Receiver<String>>>) -> Result<(), Box<dyn Error>> {
    let ws_stream = accept_async(stream).await?;
    info!("✅ New WebSocket connection established");

    let (mut write, mut read) = ws_stream.split();

    // Task to send events from AuthEventHandler to this WebSocket client
    let mut event_receiver_guard = event_rx.lock().await;
    tokio::spawn(async move {
        while let Some(event_message) = event_receiver_guard.recv().await {
            if let Err(e) = write.send(tokio_tungstenite::tungstenite::Message::text(event_message)).await {
                warn!("Failed to send event to WebSocket client: {}", e);
                break;
            }
        }
    });

    while let Some(message) = read.next().await {
        let message = message?;
        if message.is_text() {
            let text = message.to_text()?;
            info!("Received message from frontend: {}", text);

            match serde_json::from_str::<FrontendCommand>(text) {
                Ok(FrontendCommand::SubmitComment { text, episode_id }) => {
                    info!("Processing SubmitComment command for episode {}: {}", episode_id, text);
                    
                    let command = comment_it::core::UnifiedCommand::SubmitComment { text: text.clone() };
                    let public_key = PubKey(signer_keypair.x_only_public_key().0.into());
                    let episode_message = EpisodeMessage::new_signed_command(episode_id, command, signer_keypair.secret_key(), public_key);

                    // Connect to kaspad for UTXO fetching and transaction submission
                    let network_id = NetworkId::with_suffix(NetworkType::Testnet, 10);
                    let kaspad_client = connect_client(network_id, None).await?;

                    // Fetch UTXOs for the signer's address
                    let utxos = kaspad_client.get_utxos_by_addresses(vec![signer_address.clone().try_into()?]).await?.into_iter().map(|x| x.into()).collect();

                    let tx = tx_generator.build_command_transaction(utxos, &signer_address.clone().try_into()?, &episode_message, 1000); // 1000 is placeholder fee

                    match kaspad_client.submit_transaction(tx.as_ref().into(), false).await {
                        Ok(tx_id) => {
                            info!("✅ Transaction submitted: {}", tx_id);
                            let response = serde_json::to_string(&serde_json::json!({ "status": "submitted", "tx_id": tx_id }))?;
                            write.send(tokio_tungstenite::tungstenite::Message::text(response)).await?;
                        },
                        Err(e) => {
                            warn!("❌ Failed to submit transaction: {}", e);
                            let error_response = serde_json::to_string(&serde_json::json!({ "status": "error", "message": format!("Failed to submit transaction: {}", e) }))?;
                            write.send(tokio_tungstenite::tungstenite::Message::text(error_response)).await?;
                        }
                    }
                },
                Ok(FrontendCommand::RequestChallenge { episode_id }) => {
                    info!("Processing RequestChallenge command for episode {}", episode_id);
                    let command = comment_it::core::UnifiedCommand::RequestChallenge;
                    let public_key = PubKey(signer_keypair.x_only_public_key().0.into());
                    let episode_message = EpisodeMessage::new_signed_command(episode_id, command, signer_keypair.secret_key(), public_key);

                    let network_id = NetworkId::with_suffix(NetworkType::Testnet, 10);
                    let kaspad_client = connect_client(network_id, None).await?;

                    let utxos = kaspad_client.get_utxos_by_addresses(vec![signer_address.clone().try_into()?]).await?.into_iter().map(|x| x.into()).collect();

                    let tx = tx_generator.build_command_transaction(utxos, &signer_address.clone().try_into()?, &episode_message, 1000);

                    match kaspad_client.submit_transaction(tx.as_ref().into(), false).await {
                        Ok(tx_id) => {
                            info!("✅ RequestChallenge transaction submitted: {}", tx_id);
                            let response = serde_json::to_string(&serde_json::json!({ "status": "submitted", "tx_id": tx_id, "command": "RequestChallenge" }))?;
                            write.send(tokio_tungstenite::tungstenite::Message::text(response)).await?;
                        },
                        Err(e) => {
                            warn!("❌ Failed to submit RequestChallenge transaction: {}", e);
                            let error_response = serde_json::to_string(&serde_json::json!({ "status": "error", "message": format!("Failed to submit RequestChallenge: {}", e) }))?;
                            write.send(tokio_tungstenite::tungstenite::Message::text(error_response)).await?;
                        }
                    }
                },
                Ok(FrontendCommand::SubmitResponse { episode_id, signature, nonce }) => {
                    info!("Processing SubmitResponse command for episode {}", episode_id);
                    let command = comment_it::core::UnifiedCommand::SubmitResponse { signature, nonce };
                    let public_key = PubKey(signer_keypair.x_only_public_key().0.into());
                    let episode_message = EpisodeMessage::new_signed_command(episode_id, command, signer_keypair.secret_key(), public_key);

                    let network_id = NetworkId::with_suffix(NetworkType::Testnet, 10);
                    let kaspad_client = connect_client(network_id, None).await?;

                    let utxos = kaspad_client.get_utxos_by_addresses(vec![signer_address.clone().try_into()?]).await?.into_iter().map(|x| x.into()).collect();

                    let tx = tx_generator.build_command_transaction(utxos, &signer_address.clone().try_into()?, &episode_message, 1000);

                    match kaspad_client.submit_transaction(tx.as_ref().into(), false).await {
                        Ok(tx_id) => {
                            info!("✅ SubmitResponse transaction submitted: {}", tx_id);
                            let response = serde_json::to_string(&serde_json::json!({ "status": "submitted", "tx_id": tx_id, "command": "SubmitResponse" }))?;
                            write.send(tokio_tungstenite::tungstenite::Message::text(response)).await?;
                        },
                        Err(e) => {
                            warn!("❌ Failed to submit SubmitResponse transaction: {}", e);
                            let error_response = serde_json::to_string(&serde_json::json!({ "status": "error", "message": format!("Failed to submit SubmitResponse: {}", e) }))?;
                            write.send(tokio_tungstenite::tungstenite::Message::text(error_response)).await?;
                        }
                    }
                },
                Ok(FrontendCommand::RevokeSession { episode_id, signature }) => {
                    info!("Processing RevokeSession command for episode {}", episode_id);
                    let command = comment_it::core::UnifiedCommand::RevokeSession { signature };
                    let public_key = PubKey(signer_keypair.x_only_public_key().0.into());
                    let episode_message = EpisodeMessage::new_signed_command(episode_id, command, signer_keypair.secret_key(), public_key);

                    let network_id = NetworkId::with_suffix(NetworkType::Testnet, 10);
                    let kaspad_client = connect_client(network_id, None).await?;

                    let utxos = kaspad_client.get_utxos_by_addresses(vec![signer_address.clone().try_into()?]).await?.into_iter().map(|x| x.into()).collect();

                    let tx = tx_generator.build_command_transaction(utxos, &signer_address.clone().try_into()?, &episode_message, 1000);

                    match kaspad_client.submit_transaction(tx.as_ref().into(), false).await {
                        Ok(tx_id) => {
                            info!("✅ RevokeSession transaction submitted: {}", tx_id);
                            let response = serde_json::to_string(&serde_json::json!({ "status": "submitted", "tx_id": tx_id, "command": "RevokeSession" }))?;
                            write.send(tokio_tungstenite::tungstenite::Message::text(response)).await?;
                        },
                        Err(e) => {
                            warn!("❌ Failed to submit RevokeSession transaction: {}", e);
                            let error_response = serde_json::to_string(&serde_json::json!({ "status": "error", "message": format!("Failed to submit RevokeSession: {}", e) }))?;
                            write.send(tokio_tungstenite::tungstenite::Message::text(error_response)).await?;
                        }
                    }
                },
                Err(e) => {
                    warn!("Failed to parse frontend command: {}. Error: {}", text, e);
                    let error_response = serde_json::to_string(&serde_json::json!({ "status": "error", "message": format!("Invalid command: {}", e) }))?;
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
