mod episode;
mod handler;
mod program_id;
mod sim_router;
mod tlv;

use clap::Parser;
use kdapp::engine::{Engine, EngineMsg, EpisodeMessage};
use kdapp::pki::generate_keypair;

use episode::{MerchantCommand, ReceiptEpisode};
use handler::MerchantEventHandler;
use sim_router::SimRouter;

#[derive(Parser, Debug)]
#[command(name = "kdapp-merchant", version, about = "onlyKAS Merchant demo (scaffold)")]
struct Args {
    /// Run a local simulation demonstrating invoice -> pay -> ack
    #[arg(long)]
    demo: bool,
}

fn main() {
    env_logger::init();
    let args = Args::parse();

    // Engine channel wiring
    let (tx, rx) = std::sync::mpsc::channel();
    let mut engine: Engine<ReceiptEpisode, MerchantEventHandler> = Engine::new(rx);
    let handle = std::thread::spawn(move || {
        engine.start(vec![MerchantEventHandler]);
    });

    // In-process router for off-chain style delivery
    let router = SimRouter::new(tx.clone());

    // Generate a dummy merchant identity for signing episode creation
    let (merchant_sk, merchant_pk) = generate_keypair();

    // Episode id (demo constant)
    let episode_id: u32 = 42;

    // Create new episode with our merchant pubkey
    router.forward::<ReceiptEpisode>(EpisodeMessage::NewEpisode { episode_id, participants: vec![merchant_pk] });

    if args.demo {
        // Derive a simple program label to exercise helper
        let _label = program_id::derive_program_label(&merchant_pk, "merchant-pos");

        // Exercise TLV helpers to avoid dead code lints
        {
            use tlv::{hash_state, MsgType, TlvMsg, TLV_VERSION};
            let state_hash = hash_state(&[1, 2, 3]);
            let sample = TlvMsg {
                version: TLV_VERSION,
                msg_type: MsgType::New as u8,
                episode_id: episode_id as u64,
                seq: 0,
                state_hash,
                payload: vec![],
            };
            let enc = sample.encode();
            let _dec = tlv::TlvMsg::decode(&enc);
        }

        // 1) Merchant creates an invoice (signed by merchant)
        let cmd = MerchantCommand::CreateInvoice { invoice_id: 1, amount: 100_000_000, memo: Some("Latte".into()) };
        let signed = EpisodeMessage::new_signed_command(episode_id, cmd, merchant_sk, merchant_pk);
        router.forward::<ReceiptEpisode>(signed);

        // 2) Customer pays: in off-chain mode this would arrive via TLV; here push unsigned MarkPaid
        let cmd = MerchantCommand::MarkPaid { invoice_id: 1, payer: None };
        router.forward::<ReceiptEpisode>(EpisodeMessage::UnsignedCommand { episode_id, cmd });

        // 3) Merchant acknowledges receipt (signed)
        let cmd = MerchantCommand::AckReceipt { invoice_id: 1 };
        let signed = EpisodeMessage::new_signed_command(episode_id, cmd, merchant_sk, merchant_pk);
        router.forward::<ReceiptEpisode>(signed);

        // Ensure engine processes all queued messages before exit
        let _ = tx.send(EngineMsg::Exit);
        let _ = handle.join();
    }
}
