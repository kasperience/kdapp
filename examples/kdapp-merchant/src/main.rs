mod client_sender;
mod episode;
mod handler;
mod program_id;
mod scheduler;
mod server;
mod sim_router;
mod storage;
mod tcp_router;
mod tlv;
mod udp_router;
mod watcher;

use clap::{Parser, Subcommand};
use kaspa_addresses::{Address, Prefix as AddrPrefix, Version as AddrVersion};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::client::KaspaRpcClient;
use kdapp::engine::{Engine, EngineMsg, EpisodeMessage};
use kdapp::generator::{PatternType, PrefixType};
use kdapp::pki::generate_keypair;
use kdapp::pki::PubKey;
use kdapp::proxy;
use secp256k1::Keypair;
use secp256k1::SecretKey;
use std::sync::{atomic::AtomicBool, Arc};
use tokio::runtime::Runtime;

use episode::{CustomerInfo, MerchantCommand, ReceiptEpisode};
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
    RouterUdp {
        #[arg(long, default_value = "127.0.0.1:9530")]
        bind: String,
        #[arg(long, default_value_t = false)]
        proxy: bool,
    },
    /// Start a TCP TLV router that forwards TLV payloads to the engine
    RouterTcp {
        #[arg(long, default_value = "127.0.0.1:9531")]
        bind: String,
        #[arg(long, default_value_t = false)]
        proxy: bool,
    },
    /// Connect to a Kaspa node and forward accepted txs via kdapp proxy
    Proxy {
        #[arg(long)]
        merchant_private_key: Option<String>,
    },
    /// Create a new episode with the merchant public key as a participant
    New {
        #[arg(long)]
        episode_id: u32,
        #[arg(long)]
        merchant_private_key: Option<String>,
    },
    /// Create an invoice (signed by merchant)
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
        merchant_private_key: Option<String>,
    },
    /// Mark an invoice as paid (unsigned for demo)
    Pay {
        #[arg(long)]
        episode_id: u32,
        #[arg(long)]
        invoice_id: u64,
        #[arg(long)]
        payer_public_key: String,
    },
    /// Acknowledge a paid invoice (signed by merchant)
    Ack {
        #[arg(long)]
        episode_id: u32,
        #[arg(long)]
        invoice_id: u64,
        #[arg(long)]
        merchant_private_key: Option<String>,
    },
    /// Cancel an open invoice (unsigned demo)
    Cancel {
        #[arg(long)]
        episode_id: u32,
        #[arg(long)]
        invoice_id: u64,
    },
    /// Create a subscription plan for a customer (signed by merchant)
    CreateSubscription {
        #[arg(long)]
        episode_id: u32,
        #[arg(long)]
        subscription_id: u64,
        #[arg(long)]
        customer_public_key: String,
        #[arg(long)]
        amount: u64,
        #[arg(long)]
        interval: u64,
        #[arg(long)]
        merchant_private_key: Option<String>,
    },
    /// Cancel an existing subscription
    CancelSubscription {
        #[arg(long)]
        episode_id: u32,
        #[arg(long)]
        subscription_id: u64,
    },
    /// Run an HTTP server exposing merchant commands
    Serve {
        #[arg(long, default_value = "127.0.0.1:3000")]
        bind: String,
        #[arg(long)]
        episode_id: u32,
        #[arg(long)]
        api_key: String,
        #[arg(long)]
        merchant_private_key: Option<String>,
    },
    /// Build and broadcast an on-chain transaction carrying a command
    OnchainCreate {
        #[arg(long)]
        episode_id: u32,
        #[arg(long)]
        invoice_id: u64,
        #[arg(long)]
        amount: u64,
        #[arg(long)]
        memo: Option<String>,
        /// Merchant private key (signs the EpisodeMessage)
        #[arg(long)]
        merchant_private_key: Option<String>,
        /// Kaspa funding private key (signs and funds the transaction)
        #[arg(long)]
        kaspa_private_key: String,
        /// Fee in sompis (default 5_000)
        #[arg(long)]
        fee: Option<u64>,
    },
    /// Build and broadcast an on-chain transaction acknowledging a paid invoice
    OnchainAck {
        #[arg(long)]
        episode_id: u32,
        #[arg(long)]
        invoice_id: u64,
        /// Merchant private key (signs the EpisodeMessage)
        #[arg(long)]
        merchant_private_key: Option<String>,
        /// Kaspa funding private key (signs and funds the transaction)
        #[arg(long)]
        kaspa_private_key: String,
        /// Fee in sompis (default 5_000)
        #[arg(long)]
        fee: Option<u64>,
    },
    /// Register a customer and optionally supply a private key
    RegisterCustomer {
        #[arg(long)]
        customer_private_key: Option<String>,
    },
    /// List registered customers
    ListCustomers,
    /// Run a checkpoint watcher that anchors hashes on-chain
    Watch {
        #[arg(long, default_value = "127.0.0.1:9590")]
        bind: String,
        #[arg(long)]
        kaspa_private_key: String,
        #[arg(long, default_value_t = false)]
        mainnet: bool,
        #[arg(long)]
        wrpc_url: Option<String>,
    },
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

fn addr_for_keypair(keypair: &Keypair, mainnet: bool) -> Address {
    let addr_prefix = if mainnet { AddrPrefix::Mainnet } else { AddrPrefix::Testnet };
    Address::new(addr_prefix, AddrVersion::PubKey, &keypair.x_only_public_key().0.serialize())
}

async fn utxos_for_address(kaspad: &KaspaRpcClient, addr: &Address) -> Result<Vec<(TransactionOutpoint, UtxoEntry)>, String> {
    let utxos = kaspad
        .get_utxos_by_addresses(vec![addr.clone()])
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|u| (TransactionOutpoint::from(u.outpoint), UtxoEntry::from(u.utxo_entry)))
        .collect::<Vec<_>>();
    Ok(utxos)
}

async fn submit_tx_retry(kaspad: &KaspaRpcClient, tx: &kaspa_consensus_core::tx::Transaction, attempts: usize) -> Result<(), String> {
    let mut tries = 0usize;
    loop {
        match kaspad.submit_transaction(tx.into(), false).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                tries += 1;
                let msg = e.to_string();
                if tries >= attempts {
                    return Err(format!("submit failed after {tries} attempts: {msg}"));
                }
                if msg.contains("WebSocket") || msg.contains("not connected") || msg.contains("disconnected") {
                    let _ = kaspad.connect(Some(proxy::connect_options())).await;
                    continue;
                } else if msg.contains("orphan") {
                    continue;
                } else if msg.contains("already accepted") {
                    return Ok(());
                } else {
                    return Err(format!("submit failed: {msg}"));
                }
            }
        }
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
    let router = SimRouter::new(EngineChannel::Local(tx.clone()));
    match args.command.unwrap_or(CliCmd::Demo) {
        CliCmd::Demo => {
            let (merchant_sk, merchant_pk) = generate_keypair();
            let (customer_sk, customer_pk) = generate_keypair();
            storage::put_customer(&customer_pk, &CustomerInfo::default());
            let episode_id: u32 = 42;
            router.forward::<ReceiptEpisode>(EpisodeMessage::NewEpisode { episode_id, participants: vec![merchant_pk] });
            scheduler::start(router.clone(), episode_id);
            let _label = program_id::derive_program_label(&merchant_pk, "merchant-pos");
            // Create
            let cmd = MerchantCommand::CreateInvoice { invoice_id: 1, amount: 100_000_000, memo: Some("Latte".into()) };
            let signed = EpisodeMessage::new_signed_command(episode_id, cmd, merchant_sk, merchant_pk);
            router.forward::<ReceiptEpisode>(signed);
            // Pay
            let cmd = MerchantCommand::MarkPaid { invoice_id: 1, payer: customer_pk };
            router.forward::<ReceiptEpisode>(EpisodeMessage::UnsignedCommand { episode_id, cmd });
            // Ack
            let cmd = MerchantCommand::AckReceipt { invoice_id: 1 };
            let signed = EpisodeMessage::new_signed_command(episode_id, cmd, merchant_sk, merchant_pk);
            router.forward::<ReceiptEpisode>(signed);
            log::info!("demo customer private key: {}", customer_sk.display_secret());
        }
        CliCmd::RouterUdp { bind, proxy } => {
            let channel = if proxy { EngineChannel::Proxy(tx.clone()) } else { EngineChannel::Local(tx.clone()) };
            let r = udp_router::UdpRouter::new(channel);
            r.run(&bind);
        }
        CliCmd::RouterTcp { bind, proxy } => {
            let channel = if proxy { EngineChannel::Proxy(tx.clone()) } else { EngineChannel::Local(tx.clone()) };
            let r = tcp_router::TcpRouter::new(channel);
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

            let network =
                if args.mainnet { NetworkId::new(NetworkType::Mainnet) } else { NetworkId::with_suffix(NetworkType::Testnet, 10) };
            let rt = Runtime::new().expect("runtime");
            let exit = Arc::new(AtomicBool::new(false));
            let engines = std::iter::once((prefix, (pattern, tx.clone()))).collect();
            rt.block_on(async {
                let kaspad = proxy::connect_client(network, args.wrpc_url.clone()).await.expect("kaspad connect");
                proxy::run_listener(kaspad, engines, exit).await;
            });
        }
        CliCmd::New { episode_id, merchant_private_key } => {
            let (_sk, pk) = match merchant_private_key.and_then(|h| parse_secret_key(&h)) {
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
        CliCmd::Pay { episode_id, invoice_id, payer_public_key } => {
            let pk = parse_public_key(&payer_public_key).expect("invalid public key");
            let cmd = MerchantCommand::MarkPaid { invoice_id, payer: pk };
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
        CliCmd::CreateSubscription { episode_id, subscription_id, customer_public_key, amount, interval, merchant_private_key } => {
            let customer = parse_public_key(&customer_public_key).expect("invalid public key");
            let (sk, pk) = match merchant_private_key.and_then(|h| parse_secret_key(&h)) {
                Some(sk) => {
                    let pk = PubKey(secp256k1::PublicKey::from_secret_key(&secp256k1::Secp256k1::new(), &sk));
                    (sk, pk)
                }
                None => generate_keypair(),
            };
            log::info!("merchant pubkey: {pk}");
            let cmd = MerchantCommand::CreateSubscription { subscription_id, customer, amount, interval };
            let msg = EpisodeMessage::new_signed_command(episode_id, cmd, sk, pk);
            router.forward::<ReceiptEpisode>(msg);
        }
        CliCmd::CancelSubscription { episode_id, subscription_id } => {
            let cmd = MerchantCommand::CancelSubscription { subscription_id };
            let msg = EpisodeMessage::<ReceiptEpisode>::UnsignedCommand { episode_id, cmd };
            router.forward::<ReceiptEpisode>(msg);
        }
        CliCmd::Serve { bind, episode_id, api_key, merchant_private_key } => {
            let router = SimRouter::new(EngineChannel::Local(tx.clone()));
            let (sk, pk) = match merchant_private_key.and_then(|h| parse_secret_key(&h)) {
                Some(sk) => {
                    let pk = PubKey(secp256k1::PublicKey::from_secret_key(&secp256k1::Secp256k1::new(), &sk));
                    (sk, pk)
                }
                None => generate_keypair(),
            };
            log::info!("merchant pubkey: {pk}");
            scheduler::start(router.clone(), episode_id);
            let state = server::AppState::new(Arc::new(router), episode_id, sk, pk, api_key);
            let rt = Runtime::new().expect("runtime");
            rt.block_on(async {
                server::serve(bind, state).await.expect("server");
            });
        }
        CliCmd::RegisterCustomer { customer_private_key } => {
            let (sk, pk) = match customer_private_key.and_then(|h| parse_secret_key(&h)) {
                Some(sk) => {
                    let pk = PubKey(secp256k1::PublicKey::from_secret_key(&secp256k1::Secp256k1::new(), &sk));
                    (sk, pk)
                }
                None => generate_keypair(),
            };
            storage::put_customer(&pk, &CustomerInfo::default());
            println!("registered customer pubkey: {pk}");
            println!("customer private key: {}", sk.display_secret());
        }
        CliCmd::ListCustomers => {
            let customers = storage::load_customers();
            for (pk, info) in customers {
                println!("{pk}: invoices {:?} subscriptions {:?}", info.invoices, info.subscriptions);
            }
        }
        CliCmd::OnchainCreate { episode_id, invoice_id, amount, memo, merchant_private_key, kaspa_private_key, fee } => {
            let (m_sk, m_pk) = match merchant_private_key.and_then(|h| parse_secret_key(&h)) {
                Some(sk) => {
                    let pk = PubKey(secp256k1::PublicKey::from_secret_key(&secp256k1::Secp256k1::new(), &sk));
                    (sk, pk)
                }
                None => generate_keypair(),
            };
            let ids = match (args.prefix, args.pattern.as_deref().and_then(parse_pattern)) {
                (Some(pref), Some(pat)) => (pref as PrefixType, pat),
                _ => program_id::derive_routing_ids(&m_pk),
            };
            let (prefix, pattern) = ids;
            let fee = fee.unwrap_or(5_000);
            let cmd = MerchantCommand::CreateInvoice { invoice_id, amount, memo };
            let msg = EpisodeMessage::new_signed_command(episode_id, cmd, m_sk, m_pk);

            let network =
                if args.mainnet { NetworkId::new(NetworkType::Mainnet) } else { NetworkId::with_suffix(NetworkType::Testnet, 10) };
            let rt = Runtime::new().expect("runtime");
            rt.block_on(async {
                let kaspad = proxy::connect_client(network, args.wrpc_url.clone()).await.expect("kaspad connect");
                let kaspa_sk = parse_secret_key(&kaspa_private_key).expect("invalid kaspa private key");
                let keypair = Keypair::from_secret_key(&secp256k1::Secp256k1::new(), &kaspa_sk);
                let addr = addr_for_keypair(&keypair, args.mainnet);
                let utxos = utxos_for_address(&kaspad, &addr).await.expect("load utxos");
                let Some((op, entry)) = utxos.into_iter().max_by_key(|(_, e)| e.amount) else {
                    panic!("no UTXOs for address {addr:?}");
                };
                if entry.amount <= fee {
                    panic!("selected UTXO too small: {}", entry.amount);
                }
                let gen = kdapp::generator::TransactionGenerator::new(keypair, pattern, prefix);
                let tx = gen.build_command_transaction::<ReceiptEpisode>((op, entry), &addr, &msg, fee);
                let tx_id = tx.id();
                submit_tx_retry(&kaspad, &tx, 3).await.expect("submit tx");
                log::info!("on-chain create invoice submitted: tx_id={tx_id}");
            });
        }
        CliCmd::OnchainAck { episode_id, invoice_id, merchant_private_key, kaspa_private_key, fee } => {
            let (m_sk, m_pk) = match merchant_private_key.and_then(|h| parse_secret_key(&h)) {
                Some(sk) => {
                    let pk = PubKey(secp256k1::PublicKey::from_secret_key(&secp256k1::Secp256k1::new(), &sk));
                    (sk, pk)
                }
                None => generate_keypair(),
            };
            let ids = match (args.prefix, args.pattern.as_deref().and_then(parse_pattern)) {
                (Some(pref), Some(pat)) => (pref as PrefixType, pat),
                _ => program_id::derive_routing_ids(&m_pk),
            };
            let (prefix, pattern) = ids;
            let fee = fee.unwrap_or(5_000);
            let cmd = MerchantCommand::AckReceipt { invoice_id };
            let msg = EpisodeMessage::new_signed_command(episode_id, cmd, m_sk, m_pk);

            let network =
                if args.mainnet { NetworkId::new(NetworkType::Mainnet) } else { NetworkId::with_suffix(NetworkType::Testnet, 10) };
            let rt = Runtime::new().expect("runtime");
            rt.block_on(async {
                let kaspad = proxy::connect_client(network, args.wrpc_url.clone()).await.expect("kaspad connect");
                let kaspa_sk = parse_secret_key(&kaspa_private_key).expect("invalid kaspa private key");
                let keypair = Keypair::from_secret_key(&secp256k1::Secp256k1::new(), &kaspa_sk);
                let addr = addr_for_keypair(&keypair, args.mainnet);
                let utxos = utxos_for_address(&kaspad, &addr).await.expect("load utxos");
                let Some((op, entry)) = utxos.into_iter().max_by_key(|(_, e)| e.amount) else {
                    panic!("no UTXOs for address {addr:?}");
                };
                if entry.amount <= fee {
                    panic!("selected UTXO too small: {}", entry.amount);
                }
                let gen = kdapp::generator::TransactionGenerator::new(keypair, pattern, prefix);
                let tx = gen.build_command_transaction::<ReceiptEpisode>((op, entry), &addr, &msg, fee);
                let tx_id = tx.id();
                submit_tx_retry(&kaspad, &tx, 3).await.expect("submit tx");
                log::info!("on-chain ack submitted: tx_id={tx_id}");
            });
        }
        CliCmd::Watch { bind, kaspa_private_key, mainnet, wrpc_url } => {
            watcher::run(&bind, kaspa_private_key, mainnet, wrpc_url).expect("watcher");
        }
    }

    // Ensure engine processes all queued messages before exit
    let _ = tx.send(EngineMsg::Exit);
    let _ = handle.join();
}
