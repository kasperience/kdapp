mod client_sender;
mod episode;
mod pki;
mod tlv;

use clap::{Parser, Subcommand};
use episode::{MerchantCommand, ReceiptEpisode};
use kdapp::engine::EpisodeMessage;
use kdapp::pki::PubKey;
use kdapp_guardian::{self as guardian};
use secp256k1::{Secp256k1, SecretKey};
use serde::Deserialize;

use client_sender::{handshake_on, send_cmd_on, send_new_on};
use std::net::UdpSocket;
use tlv::{DEMO_HMAC_KEY, SCRIPT_POLICY_VERSION};

#[derive(Parser, Debug)]
#[command(name = "kdapp-customer", about = "Interact with merchant invoices")]
struct Args {
    #[arg(long, default_value = "127.0.0.1:9530")]
    dest: String,
    #[arg(long, default_value = "http://127.0.0.1:3000")]
    server: String,
    #[arg(long)]
    api_key: Option<String>,
    #[arg(long = "guardian-addr")]
    guardian_addr: Vec<String>,
    #[arg(long = "guardian-key")]
    guardian_public_key: Vec<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// List invoices via HTTP
    List,
    /// Create an invoice via TLV (signed by merchant)
    Create {
        #[arg(long)]
        episode_id: u32,
        #[arg(long)]
        invoice_id: u64,
        #[arg(long)]
        amount: u64,
        #[arg(long)]
        memo: Option<String>,
        #[arg(long)]
        merchant_private_key: String,
    },
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

fn parse_public_key(hex: &str) -> Option<PubKey> {
    let mut buf = [0u8; 33];
    let mut tmp = vec![0u8; hex.len() / 2 + hex.len() % 2];
    if faster_hex::hex_decode(hex.as_bytes(), &mut tmp).is_ok() && tmp.len() == 33 {
        buf.copy_from_slice(&tmp);
        secp256k1::PublicKey::from_slice(&buf).ok().map(PubKey)
    } else {
        None
    }
}

fn warn_if_outdated(remote_version: Option<u16>) {
    if let Some(remote) = remote_version {
        if remote > SCRIPT_POLICY_VERSION {
            log::warn!(
                "router requires script policy v{} but client only supports v{}; upgrade kdapp-customer",
                remote,
                SCRIPT_POLICY_VERSION
            );
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = Args::parse();
    let guardians: Vec<(String, PubKey)> = args
        .guardian_addr
        .iter()
        .zip(args.guardian_public_key.iter())
        .filter_map(|(addr, pk_hex)| parse_public_key(pk_hex).map(|pk| (addr.clone(), pk)))
        .collect();
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
                            println!(
                                "invoice {} amount {} status {} memo {:?} payer {:?} created_at {} updated {}",
                                inv.id, inv.amount, inv.status, inv.memo, inv.payer, inv.created_at, inv.last_update
                            );
                        }
                    }
                    Err(e) => eprintln!("list failed (decode): {e}"),
                },
                Err(e) => eprintln!("list failed (request): {e}"),
            }
        }
        Command::Create { episode_id, invoice_id, amount, memo, merchant_private_key } => {
            let sock = UdpSocket::bind("0.0.0.0:0").expect("bind sender");
            warn_if_outdated(handshake_on(&sock, &args.dest, DEMO_HMAC_KEY));
            let sk = parse_secret_key(&merchant_private_key).expect("invalid private key");
            let secp = Secp256k1::new();
            let pk = PubKey(secp256k1::PublicKey::from_secret_key(&secp, &sk));
            for (addr, gpk) in &guardians {
                guardian::handshake(addr, pk, *gpk, guardian::DEMO_HMAC_KEY);
            }
            let new_msg = EpisodeMessage::<ReceiptEpisode>::NewEpisode { episode_id, participants: vec![pk] };
            send_new_on(&sock, &args.dest, episode_id as u64, 0, new_msg, DEMO_HMAC_KEY);
            let gkeys = guardians.iter().map(|(_, pk)| *pk).collect();
            let cmd = MerchantCommand::CreateInvoice { invoice_id, amount, memo, guardian_keys: gkeys };
            let msg = EpisodeMessage::new_signed_command(episode_id, cmd, sk, pk);
            send_cmd_on(&sock, &args.dest, episode_id as u64, 1, msg, DEMO_HMAC_KEY);
            for (addr, _) in &guardians {
                guardian::send_confirm(addr, episode_id as u64, 1, guardian::DEMO_HMAC_KEY);
            }
        }
        Command::Pay { episode_id, invoice_id, payer_private_key } => {
            // Use one UDP socket for handshake + subsequent signed messages (stable src addr)
            let sock = UdpSocket::bind("0.0.0.0:0").expect("bind sender");
            warn_if_outdated(handshake_on(&sock, &args.dest, DEMO_HMAC_KEY));
            let sk = parse_secret_key(&payer_private_key).expect("invalid private key");
            let secp = Secp256k1::new();
            let pk = PubKey(secp256k1::PublicKey::from_secret_key(&secp, &sk));
            for (addr, gpk) in &guardians {
                guardian::handshake(addr, pk, *gpk, guardian::DEMO_HMAC_KEY);
            }
            let new_msg = EpisodeMessage::<ReceiptEpisode>::NewEpisode { episode_id, participants: vec![pk] };
            send_new_on(&sock, &args.dest, episode_id as u64, 0, new_msg, DEMO_HMAC_KEY);
            let cmd = MerchantCommand::MarkPaid { invoice_id, payer: pk };
            let msg = EpisodeMessage::new_signed_command(episode_id, cmd, sk, pk);
            send_cmd_on(&sock, &args.dest, episode_id as u64, 1, msg, DEMO_HMAC_KEY);
            for (addr, _) in &guardians {
                guardian::send_confirm(addr, episode_id as u64, 1, guardian::DEMO_HMAC_KEY);
            }
        }
        Command::Ack { episode_id, invoice_id, merchant_private_key } => {
            // Use one UDP socket for handshake + subsequent signed messages (stable src addr)
            let sock = UdpSocket::bind("0.0.0.0:0").expect("bind sender");
            warn_if_outdated(handshake_on(&sock, &args.dest, DEMO_HMAC_KEY));
            let sk = parse_secret_key(&merchant_private_key).expect("invalid private key");
            let secp = Secp256k1::new();
            let pk = PubKey(secp256k1::PublicKey::from_secret_key(&secp, &sk));
            for (addr, gpk) in &guardians {
                guardian::handshake(addr, pk, *gpk, guardian::DEMO_HMAC_KEY);
            }
            let new_msg = EpisodeMessage::<ReceiptEpisode>::NewEpisode { episode_id, participants: vec![pk] };
            send_new_on(&sock, &args.dest, episode_id as u64, 0, new_msg, DEMO_HMAC_KEY);
            let cmd = MerchantCommand::AckReceipt { invoice_id };
            let msg = EpisodeMessage::new_signed_command(episode_id, cmd, sk, pk);
            send_cmd_on(&sock, &args.dest, episode_id as u64, 1, msg, DEMO_HMAC_KEY);
            for (addr, _) in &guardians {
                guardian::send_confirm(addr, episode_id as u64, 1, guardian::DEMO_HMAC_KEY);
            }
        }
    }
}
