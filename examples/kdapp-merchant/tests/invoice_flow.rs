#[path = "../../kdapp-customer/src/episode.rs"]
mod customer_episode;
mod fixtures;

use customer_episode::{
    InvoiceStatus as CustomerInvoiceStatus, MerchantCommand as CustomerCommand, ReceiptEpisode as CustomerEpisode,
};
use fixtures::episode::{InvoiceStatus, MerchantCommand, MerchantError};
use fixtures::setup;
use kdapp::episode::{Episode, EpisodeError, TxOutputInfo};
use kdapp_guardian::{receive, send_confirm, send_escalate, GuardianMsg, GuardianState, DEMO_HMAC_KEY};
use std::net::UdpSocket;
use std::thread;

#[test]
fn invoice_flow_with_guardian() {
    let mut ctx = setup();
    let mut customer = CustomerEpisode::initialize(vec![ctx.customer], &ctx.metadata);

    let create = MerchantCommand::CreateInvoice { invoice_id: 1, amount: 100, memo: Some("coffee".into()) };
    ctx.episode.execute(&create, Some(ctx.merchant), &ctx.metadata).expect("merchant create");
    let c_create = CustomerCommand::CreateInvoice { invoice_id: 1, amount: 100, memo: Some("coffee".into()) };
    customer.execute(&c_create, Some(ctx.merchant), &ctx.metadata).expect("customer create");

    let script = {
        let mut s = Vec::with_capacity(35);
        s.push(33);
        s.extend_from_slice(&ctx.merchant.0.serialize());
        s.push(0xac);
        s
    };
    let mut md_paid = ctx.metadata.clone();
    md_paid.tx_outputs = Some(vec![TxOutputInfo { value: 100, script_version: 0, script_bytes: Some(script) }]);
    let pay = MerchantCommand::MarkPaid { invoice_id: 1, payer: ctx.customer };
    ctx.episode.execute(&pay, Some(ctx.customer), &md_paid).expect("merchant paid");
    let c_pay = CustomerCommand::MarkPaid { invoice_id: 1, payer: ctx.customer };
    customer.execute(&c_pay, Some(ctx.customer), &md_paid).expect("customer paid");

    let ack = MerchantCommand::AckReceipt { invoice_id: 1 };
    ctx.episode.execute(&ack, Some(ctx.merchant), &ctx.metadata).expect("merchant ack");
    let c_ack = CustomerCommand::AckReceipt { invoice_id: 1 };
    customer.execute(&c_ack, Some(ctx.merchant), &ctx.metadata).expect("customer ack");
    assert!(matches!(ctx.episode.invoices.get(&1).unwrap().status, InvoiceStatus::Acked));
    assert!(matches!(customer.invoice(1).unwrap().status, CustomerInvoiceStatus::Acked));

    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    let addr = server.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let mut state = GuardianState::default();
        let msg1 = receive(&server, &mut state, DEMO_HMAC_KEY).unwrap();
        assert!(matches!(msg1, GuardianMsg::Escalate { episode_id: 1, .. }));
        let msg2 = receive(&server, &mut state, DEMO_HMAC_KEY).unwrap();
        assert!(matches!(msg2, GuardianMsg::Confirm { episode_id: 1, seq: 7 }));
        state
    });
    send_escalate(&addr.to_string(), 1, "late payment".into(), vec![], DEMO_HMAC_KEY);
    send_confirm(&addr.to_string(), 1, 7, DEMO_HMAC_KEY);
    let state = handle.join().unwrap();
    assert_eq!(state.observed_payments, vec![1]);
    assert_eq!(state.checkpoints, vec![(1, 7)]);
}

#[test]
fn replay_attack_rejected() {
    let mut ctx = setup();
    let mut customer = CustomerEpisode::initialize(vec![ctx.customer], &ctx.metadata);
    for id in [1, 2] {
        let cmd = MerchantCommand::CreateInvoice { invoice_id: id, amount: 50, memo: None };
        ctx.episode.execute(&cmd, Some(ctx.merchant), &ctx.metadata).unwrap();
        let c_cmd = CustomerCommand::CreateInvoice { invoice_id: id, amount: 50, memo: None };
        customer.execute(&c_cmd, Some(ctx.merchant), &ctx.metadata).unwrap();
    }
    let script = {
        let mut s = Vec::with_capacity(35);
        s.push(33);
        s.extend_from_slice(&ctx.merchant.0.serialize());
        s.push(0xac);
        s
    };
    let mut md = ctx.metadata.clone();
    md.tx_outputs = Some(vec![TxOutputInfo { value: 50, script_version: 0, script_bytes: Some(script) }]);
    ctx.episode.execute(&MerchantCommand::MarkPaid { invoice_id: 1, payer: ctx.customer }, Some(ctx.customer), &md).unwrap();
    let err =
        ctx.episode.execute(&MerchantCommand::MarkPaid { invoice_id: 2, payer: ctx.customer }, Some(ctx.customer), &md).unwrap_err();
    match err {
        EpisodeError::InvalidCommand(MerchantError::DuplicatePayment) => {}
        _ => panic!("expected duplicate payment"),
    }
}

#[test]
fn incorrect_payment_amount_rejected() {
    let mut ctx = setup();
    let mut customer = CustomerEpisode::initialize(vec![ctx.customer], &ctx.metadata);
    let create = MerchantCommand::CreateInvoice { invoice_id: 3, amount: 100, memo: None };
    ctx.episode.execute(&create, Some(ctx.merchant), &ctx.metadata).unwrap();
    let c_create = CustomerCommand::CreateInvoice { invoice_id: 3, amount: 100, memo: None };
    customer.execute(&c_create, Some(ctx.merchant), &ctx.metadata).unwrap();
    let script = {
        let mut s = Vec::with_capacity(35);
        s.push(33);
        s.extend_from_slice(&ctx.merchant.0.serialize());
        s.push(0xac);
        s
    };
    let mut md = ctx.metadata.clone();
    md.tx_outputs = Some(vec![TxOutputInfo { value: 90, script_version: 0, script_bytes: Some(script) }]);
    let err =
        ctx.episode.execute(&MerchantCommand::MarkPaid { invoice_id: 3, payer: ctx.customer }, Some(ctx.customer), &md).unwrap_err();
    match err {
        EpisodeError::InvalidCommand(MerchantError::InvalidAmount) => {}
        _ => panic!("expected invalid amount"),
    }
}
