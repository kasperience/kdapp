mod episode;
mod handler;
mod program_id;
mod sim_router;
mod udp_router;
mod tlv;
mod storage;

use clap::{Parser, Subcommand};
use kdapp::engine::{Engine, EngineMsg, EpisodeMessage};
use kdapp::pki::generate_keypair;
use kdapp::pki::PubKey;
use secp256k1::SecretKey;

use episode::{MerchantCommand, ReceiptEpisode};
use handler::MerchantEventHandler;
use sim_router::SimRouter;

#[derive(Parser, Debug)]
#[command(name = "kdapp-merchant", version, about = "onlyKAS Merchant demo (scaffold)")]
struct Args {
    #[command(subcommand)]
    command: Option<CliCmd>,
}

#[derive(Subcommand, Debug)]
enum CliCmd {
    /// Run the original demo flow in-process
    Demo,
    /// Start a UDP TLV router that forwards TLV payloads to the engine
    RouterUdp { #[arg(long, default_value = "127.0.0.1:9530")] bind: String },
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
    let router = SimRouter::new(tx.clone());
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
        CliCmd::RouterUdp { bind } => {
            let r = udp_router::UdpRouter::new(tx.clone());
            r.run(&bind);
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
