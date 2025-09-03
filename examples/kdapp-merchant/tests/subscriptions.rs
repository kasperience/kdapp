mod fixtures;
use fixtures::{create_subscription, episode::{MerchantCommand, MerchantError}, setup};
use kdapp::episode::EpisodeError;

#[test]
fn subscription_creation_and_recurring_charges() {
    let mut ctx = setup();
    create_subscription(&mut ctx, 1, 100, 10);
    let next = ctx.metadata.accepting_time + 10;
    assert_eq!(ctx.episode.subscriptions.get(&1).unwrap().next_run, next);

    // process twice to simulate recurring charges
    let mut md = ctx.metadata.clone();
    md.accepting_time = next;
    ctx
        .episode
        .execute(&MerchantCommand::ProcessSubscription { subscription_id: 1 }, None, &md)
        .expect("process once");
    assert_eq!(ctx.episode.subscriptions.get(&1).unwrap().next_run, next + 10);

    md.accepting_time = next + 10;
    ctx
        .episode
        .execute(&MerchantCommand::ProcessSubscription { subscription_id: 1 }, None, &md)
        .expect("process twice");
    assert_eq!(ctx.episode.subscriptions.get(&1).unwrap().next_run, next + 20);
}

#[test]
fn subscription_failure_paths() {
    let mut ctx = setup();

    // creating two subscriptions with the same id should fail
    create_subscription(&mut ctx, 2, 50, 5);
    let err = ctx
        .episode
        .execute(
            &MerchantCommand::CreateSubscription {
                subscription_id: 2,
                customer: ctx.customer,
                amount: 50,
                interval: 5,
            },
            Some(ctx.merchant),
            &ctx.metadata,
        )
        .unwrap_err();
    match err {
        EpisodeError::InvalidCommand(MerchantError::SubscriptionExists) => {}
        _ => panic!("expected duplicate subscription error"),
    }

    // processing a non-existent subscription should fail
    let err = ctx
        .episode
        .execute(
            &MerchantCommand::ProcessSubscription { subscription_id: 999 },
            None,
            &ctx.metadata,
        )
        .unwrap_err();
    match err {
        EpisodeError::InvalidCommand(MerchantError::SubscriptionNotFound) => {}
        _ => panic!("expected missing subscription error"),
    }
}
