mod episode;
mod handler;
mod program_id;
mod sim_router;
mod udp_router;
mod tlv;
mod storage;

use clap::{Parser, Subcommand};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kdapp::engine::{Engine, EngineMsg, EpisodeMessage};
use kdapp::generator::{PatternType, PrefixType};
use kdapp::pki::generate_keypair;
use kdapp::pki::PubKey;
use kdapp::proxy;
use secp256k1::SecretKey;
use std::sync::{atomic::AtomicBool, Arc};
use tokio::runtime::Runtime;

use episode::{MerchantCommand, ReceiptEpisode};
use handler::MerchantEventHandler;
use sim_router::{EngineChannel, SimRouter};

#[derive(Parser, Debug)]
#[command(name = "kdapp-merchant", version, about = "onlyKAS Merchant demo (scaffold)")]
struct Args {
    /// Optional wRPC endpoint; defaults to public node network
    #[arg(long)]
    wrpc_url: Option<String>,
    /// Use mainnet (default testnet-10)
    #[arg(long, default_value_t = false)]
    mainnet: bool,
    /// Override routing prefix
    #[arg(long)]
    prefix: Option<u32>,
    /// Override routing pattern as "pos:bit,pos:bit,..."
    #[arg(long)]
    pattern: Option<String>,
    #[command(subcommand)]
    command: Option<CliCmd>,
}

#[derive(Subcommand, Debug)]
enum CliCmd {
    /// Run the original demo flow in-process
    Demo,
    /// Start a UDP TLV router that forwards TLV payloads to the engine
    RouterUdp { #[arg(long, default_value = "127.0.0.1:9530")] bind: String, #[arg(long, default_value_t = false)] proxy: bool },
    /// Connect to a Kaspa node and forward accepted txs via kdapp proxy
    Proxy { #[arg(long)] merchant_private_key: Option<String> },
    /// Create a new episode with the merchant public key as a participant
    New { #[arg(long)] episode_id: u32, #[arg(long)] merchant_private_key: Option<String> },
    /// Create an invoice (signed by merchant)
    Create {
        #[arg(long)] episode_id: u32,
        #[arg(long)] invoice_id: u64,
        #[arg(long)] amount: u64,
        #[arg(long)] memo: Option<String>,
        #[arg(long)] merchant_private_key: Option<String>,
    },
    /// Mark an invoice as paid (unsigned for demo)
    Pay { #[arg(long)] episode_id: u32, #[arg(long)] invoice_id: u64 },
    /// Acknowledge a paid invoice (signed by merchant)
    Ack { #[arg(long)] episode_id: u32, #[arg(long)] invoice_id: u64, #[arg(long)] merchant_private_key: Option<String> },
    /// Cancel an open invoice (unsigned demo)
    Cancel { #[arg(long)] episode_id: u32, #[arg(long)] invoice_id: u64 },
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

fn parse_pattern(s: &str) -> Option<PatternType> {
    let mut out = [(0u8, 0u8); 10];
    let parts: Vec<_> = s.split(',').collect();
    if parts.len() != 10 {
        return None;
    }
    for (i, part) in parts.iter().enumerate() {
        let (p, b) = part.split_once(':')?;
        let pos = p.parse().ok()?;
        let bit = b.parse().ok()?;
        out[i] = (pos, bit);
    }
    Some(out)
}

fn main() {
    env_logger::init();
    storage::init();
    let args = Args::parse();

    // Engine channel wiring
    let (tx, rx) = std::sync::mpsc::channel();
    let mut engine: Engine<ReceiptEpisode, MerchantEventHandler> = Engine::new(rx);
    let handle = std::thread::spawn(move || {
        engine.start(vec![MerchantEventHandler]);
    });

    // In-process router for off-chain style delivery
    let router = SimRouter::new(EngineChannel::Local(tx.clone()));
    match args.command.unwrap_or(CliCmd::Demo) {
        CliCmd::Demo => {
            let (merchant_sk, merchant_pk) = generate_keypair();
            let episode_id: u32 = 42;
            router.forward::<ReceiptEpisode>(EpisodeMessage::NewEpisode { episode_id, participants: vec![merchant_pk] });
            let _label = program_id::derive_program_label(&merchant_pk, "merchant-pos");
            // Create
            let cmd = MerchantCommand::CreateInvoice { invoice_id: 1, amount: 100_000_000, memo: Some("Latte".into()) };
            let signed = EpisodeMessage::new_signed_command(episode_id, cmd, merchant_sk, merchant_pk);
            router.forward::<ReceiptEpisode>(signed);
            // Pay
            let cmd = MerchantCommand::MarkPaid { invoice_id: 1, payer: None };
            router.forward::<ReceiptEpisode>(EpisodeMessage::UnsignedCommand { episode_id, cmd });
            // Ack
            let cmd = MerchantCommand::AckReceipt { invoice_id: 1 };
            let signed = EpisodeMessage::new_signed_command(episode_id, cmd, merchant_sk, merchant_pk);
            router.forward::<ReceiptEpisode>(signed);
        }
        CliCmd::RouterUdp { bind, proxy } => {
            let channel = if proxy { EngineChannel::Proxy(tx.clone()) } else { EngineChannel::Local(tx.clone()) };
            let r = udp_router::UdpRouter::new(channel);
            r.run(&bind);
        }
        CliCmd::Proxy { merchant_private_key } => {
            let (_sk, pk) = match merchant_private_key.and_then(|h| parse_secret_key(&h)) {
                Some(sk) => {
                    let pk = PubKey(secp256k1::PublicKey::from_secret_key(&secp256k1::Secp256k1::new(), &sk));
                    (sk, pk)
                }
                None => generate_keypair(),
            };
            log::info!("merchant pubkey: {pk}");
            let ids = match (args.prefix, args.pattern.as_deref().and_then(parse_pattern)) {
                (Some(pref), Some(pat)) => (pref as PrefixType, pat),
                _ => program_id::derive_routing_ids(&pk),
            };
            let (prefix, pattern) = ids;
            log::info!("prefix=0x{prefix:08x}, pattern={pattern:?}");

            let network = if args.mainnet {
                NetworkId::new(NetworkType::Mainnet)
            } else {
                NetworkId::with_suffix(NetworkType::Testnet, 10)
            };
            let rt = Runtime::new().expect("runtime");
            let exit = Arc::new(AtomicBool::new(false));
            let engines = std::iter::once((prefix, (pattern, tx.clone()))).collect();
            rt.block_on(async {
                let kaspad = proxy::connect_client(network, args.wrpc_url.clone())
                    .await
                    .expect("kaspad connect");
                proxy::run_listener(kaspad, engines, exit).await;
            });
        }
        CliCmd::New { episode_id, merchant_private_key } => {
            let (sk, pk) = match merchant_private_key.and_then(|h| parse_secret_key(&h)) {
                Some(sk) => {
                    let pk = PubKey(secp256k1::PublicKey::from_secret_key(&secp256k1::Secp256k1::new(), &sk));
                    (sk, pk)
                }
                None => generate_keypair(),
            };
            log::info!("merchant pubkey: {pk}");
            let msg = EpisodeMessage::<ReceiptEpisode>::NewEpisode { episode_id, participants: vec![pk] };
            router.forward::<ReceiptEpisode>(msg);
        }
        CliCmd::Create { episode_id, invoice_id, amount, memo, merchant_private_key } => {
            let (sk, pk) = match merchant_private_key.and_then(|h| parse_secret_key(&h)) {
                Some(sk) => {
                    let pk = PubKey(secp256k1::PublicKey::from_secret_key(&secp256k1::Secp256k1::new(), &sk));
                    (sk, pk)
                }
                None => generate_keypair(),
            };
            log::info!("merchant pubkey: {pk}");
            let cmd = MerchantCommand::CreateInvoice { invoice_id, amount, memo };
            let msg = EpisodeMessage::new_signed_command(episode_id, cmd, sk, pk);
            router.forward::<ReceiptEpisode>(msg);
        }
        CliCmd::Pay { episode_id, invoice_id } => {
            let cmd = MerchantCommand::MarkPaid { invoice_id, payer: None };
            let msg = EpisodeMessage::<ReceiptEpisode>::UnsignedCommand { episode_id, cmd };
            router.forward::<ReceiptEpisode>(msg);
        }
        CliCmd::Ack { episode_id, invoice_id, merchant_private_key } => {
            let (sk, pk) = match merchant_private_key.and_then(|h| parse_secret_key(&h)) {
                Some(sk) => {
                    let pk = PubKey(secp256k1::PublicKey::from_secret_key(&secp256k1::Secp256k1::new(), &sk));
                    (sk, pk)
                }
                None => generate_keypair(),
            };
            log::info!("merchant pubkey: {pk}");
            let cmd = MerchantCommand::AckReceipt { invoice_id };
            let msg = EpisodeMessage::new_signed_command(episode_id, cmd, sk, pk);
            router.forward::<ReceiptEpisode>(msg);
        }
        CliCmd::Cancel { episode_id, invoice_id } => {
            let cmd = MerchantCommand::CancelInvoice { invoice_id };
            let msg = EpisodeMessage::<ReceiptEpisode>::UnsignedCommand { episode_id, cmd };
            router.forward::<ReceiptEpisode>(msg);
        }
    }

    // Ensure engine processes all queued messages before exit
    let _ = tx.send(EngineMsg::Exit);
    let _ = handle.join();
}
