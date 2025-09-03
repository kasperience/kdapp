#[path = "../src/episode.rs"]
pub mod episode;
#[path = "../src/storage.rs"]
pub mod storage;

use episode::{CustomerInfo, MerchantCommand, ReceiptEpisode};
use kdapp::episode::PayloadMetadata;
use kdapp::pki::{generate_keypair, PubKey};
use kaspa_consensus_core::Hash;

/// Test context containing a merchant episode with one registered customer.
pub struct TestContext {
    pub episode: ReceiptEpisode,
    pub merchant: PubKey,
    pub customer: PubKey,
    pub metadata: PayloadMetadata,
}

/// Initialize sled storage and create a fresh episode with a merchant and customer.
pub fn setup() -> TestContext {
    // ensure a clean database for each test run
    let _ = std::fs::remove_dir_all("merchant.db");
    storage::init();

    let (_sk_m, merchant_pk) = generate_keypair();
    let (_sk_c, customer_pk) = generate_keypair();
    storage::put_customer(&customer_pk, &CustomerInfo::default());

    let metadata = PayloadMetadata {
        accepting_hash: Hash::default(),
        accepting_daa: 0,
        accepting_time: 1,
        tx_id: Hash::default(),
        tx_outputs: None,
    };

    let episode = ReceiptEpisode::initialize(vec![merchant_pk], &metadata);

    TestContext { episode, merchant: merchant_pk, customer: customer_pk, metadata }
}

/// Helper for creating a subscription in tests.
pub fn create_subscription(ctx: &mut TestContext, id: u64, amount: u64, interval: u64) {
    let cmd = MerchantCommand::CreateSubscription {
        subscription_id: id,
        customer: ctx.customer,
        amount,
        interval,
    };
    ctx
        .episode
        .execute(&cmd, Some(ctx.merchant), &ctx.metadata)
        .expect("create subscription");
}
