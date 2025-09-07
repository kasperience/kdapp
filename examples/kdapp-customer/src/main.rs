mod client_sender;
mod episode;
mod tlv;

use clap::{Parser, Subcommand};
use episode::{MerchantCommand, ReceiptEpisode};
use kdapp::engine::EpisodeMessage;
use kdapp::pki::PubKey;
use secp256k1::{Secp256k1, SecretKey};
use serde::Deserialize;

use client_sender::{handshake, send_cmd, send_new};
use tlv::DEMO_HMAC_KEY;

#[derive(Parser, Debug)]
#[command(name = "kdapp-customer", about = "Interact with merchant invoices")]
struct Args {
    #[arg(long, default_value = "127.0.0.1:9530")]
    dest: String,
    #[arg(long, default_value = "http://127.0.0.1:3000")]
    server: String,
    #[arg(long)]
    api_key: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// List invoices via HTTP
    List,
    /// Pay an invoice using TLV transport
    Pay {
        #[arg(long)]
        episode_id: u32,
        #[arg(long)]
        invoice_id: u64,
        #[arg(long)]
        payer_private_key: String,
    },
    /// Acknowledge a paid invoice using TLV transport
    Ack {
        #[arg(long)]
        episode_id: u32,
        #[arg(long)]
        invoice_id: u64,
        #[arg(long)]
        merchant_private_key: String,
    },
}

#[derive(Deserialize)]
struct InvoiceOut {
    id: u64,
    amount: u64,
    memo: Option<String>,
    status: String,
    payer: Option<String>,
    created_at: u64,
    last_update: u64,
}

fn parse_secret_key(hex: &str) -> Option<SecretKey> {
    let mut buf = [0u8; 32];
    let mut tmp = vec![0u8; hex.len() / 2 + hex.len() % 2];
    if faster_hex::hex_decode(hex.as_bytes(), &mut tmp).is_ok() && tmp.len() == 32 {
        buf.copy_from_slice(&tmp);
        SecretKey::from_slice(&buf).ok()
    } else {
        None
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = Args::parse();
    match args.command {
        Command::List => {
            let client = reqwest::Client::new();
            let mut req = client.get(format!("{}/invoices", args.server));
            if let Some(key) = args.api_key.as_deref() {
                req = req.header("x-api-key", key);
            }
            match req.send().await {
                Ok(resp) => match resp.json::<Vec<InvoiceOut>>().await {
                    Ok(invoices) => {
                        for inv in invoices {
                            println!("invoice {} amount {} status {}", inv.id, inv.amount, inv.status);
                        }
                    }
                    Err(e) => eprintln!("list failed (decode): {e}"),
                },
                Err(e) => eprintln!("list failed (request): {e}"),
            }
        }
        Command::Pay { episode_id, invoice_id, payer_private_key } => {
            // Establish per-destination key before sending signed messages
            handshake(&args.dest, DEMO_HMAC_KEY);
            let sk = parse_secret_key(&payer_private_key).expect("invalid private key");
            let secp = Secp256k1::new();
            let pk = PubKey(secp256k1::PublicKey::from_secret_key(&secp, &sk));
            let new_msg = EpisodeMessage::<ReceiptEpisode>::NewEpisode { episode_id, participants: vec![pk] };
            send_new(&args.dest, episode_id as u64, 0, new_msg, DEMO_HMAC_KEY);
            let cmd = MerchantCommand::MarkPaid { invoice_id, payer: pk };
            let msg = EpisodeMessage::new_signed_command(episode_id, cmd, sk, pk);
            send_cmd(&args.dest, episode_id as u64, 1, msg, DEMO_HMAC_KEY);
        }
        Command::Ack { episode_id, invoice_id, merchant_private_key } => {
            // Establish per-destination key before sending signed messages
            handshake(&args.dest, DEMO_HMAC_KEY);
            let sk = parse_secret_key(&merchant_private_key).expect("invalid private key");
            let secp = Secp256k1::new();
            let pk = PubKey(secp256k1::PublicKey::from_secret_key(&secp, &sk));
            let new_msg = EpisodeMessage::<ReceiptEpisode>::NewEpisode { episode_id, participants: vec![pk] };
            send_new(&args.dest, episode_id as u64, 0, new_msg, DEMO_HMAC_KEY);
            let cmd = MerchantCommand::AckReceipt { invoice_id };
            let msg = EpisodeMessage::new_signed_command(episode_id, cmd, sk, pk);
            send_cmd(&args.dest, episode_id as u64, 1, msg, DEMO_HMAC_KEY);
        }
    }
}
