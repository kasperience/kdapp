mod fixtures;
use fixtures::{
    create_subscription,
    episode::{MerchantCommand, MerchantError},
    setup,
};
use kdapp::episode::Episode;
use kdapp::episode::EpisodeError;

#[test]
fn subscription_creation_and_recurring_charges() {
    let mut ctx = setup();
    let interval = 10u64;
    create_subscription(&mut ctx, 1, 100, interval);
    let expected = ctx.metadata.accepting_time + interval;
    let jitter = std::cmp::max(1, interval * 5 / 100);
    let first_run = ctx.episode.subscriptions.get(&1).unwrap().next_run_ts;
    assert!(first_run >= expected - jitter && first_run <= expected + jitter);

    // process twice to simulate recurring charges
    let mut md = ctx.metadata.clone();
    md.accepting_time = first_run;
    ctx.episode.execute(&MerchantCommand::ProcessSubscription { subscription_id: 1 }, None, &md).expect("process once");
    let second_expected = first_run + interval;
    let second_run = ctx.episode.subscriptions.get(&1).unwrap().next_run_ts;
    assert!(second_run >= second_expected - jitter && second_run <= second_expected + jitter);

    md.accepting_time = second_run;
    ctx.episode.execute(&MerchantCommand::ProcessSubscription { subscription_id: 1 }, None, &md).expect("process twice");
    let third_expected = second_run + interval;
    let third_run = ctx.episode.subscriptions.get(&1).unwrap().next_run_ts;
    assert!(third_run >= third_expected - jitter && third_run <= third_expected + jitter);
}

#[test]
fn subscription_failure_paths() {
    let mut ctx = setup();

    // creating two subscriptions with the same id should fail
    create_subscription(&mut ctx, 2, 50, 5);
    let err = ctx
        .episode
        .execute(
            &MerchantCommand::CreateSubscription { subscription_id: 2, customer: ctx.customer, amount: 50, interval: 5 },
            Some(ctx.merchant),
            &ctx.metadata,
        )
        .unwrap_err();
    match err {
        EpisodeError::InvalidCommand(MerchantError::SubscriptionExists) => {}
        _ => panic!("expected duplicate subscription error"),
    }

    // processing a non-existent subscription should fail
    let err = ctx.episode.execute(&MerchantCommand::ProcessSubscription { subscription_id: 999 }, None, &ctx.metadata).unwrap_err();
    match err {
        EpisodeError::InvalidCommand(MerchantError::SubscriptionNotFound) => {}
        _ => panic!("expected missing subscription error"),
    }
}
