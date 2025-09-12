use std::collections::BTreeMap;

use crate::pki::p2pk_script;
use borsh::{BorshDeserialize, BorshSerialize};
use kdapp::episode::{Episode, EpisodeError as KdappEpisodeError, PayloadMetadata};
use kdapp::pki::PubKey;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// Must mirror the wire shape used by kdapp-merchant to ensure Borsh compatibility
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum MerchantCommand {
    CreateInvoice { invoice_id: u64, amount: u64, memo: Option<String>, guardian_keys: Vec<PubKey> },
    MarkPaid { invoice_id: u64, payer: PubKey },
    AckReceipt { invoice_id: u64 },
    CancelInvoice { invoice_id: u64 },
    CreateSubscription { subscription_id: u64, customer: PubKey, amount: u64, interval: u64 },
    ProcessSubscription { subscription_id: u64 },
    CancelSubscription { subscription_id: u64 },
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub enum InvoiceStatus {
    Open,
    Paid,
    Acked,
    Canceled,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Invoice {
    pub id: u64,
    pub amount: u64,
    pub memo: Option<String>,
    pub status: InvoiceStatus,
    pub payer: Option<PubKey>,
    pub merchant_pubkey: PubKey,
    pub guardian_keys: Vec<PubKey>,
}

#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, PartialEq, Eq)]
pub enum SubStatus {
    Active,
    Paused,
    Canceled,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct Subscription {
    pub sub_id: u64,
    pub id: u64,
    pub customer_id: u64,
    pub merchant_pubkey: PubKey,
    pub amount_sompi: u64,
    pub period_secs: u64,
    pub next_run_ts: u64,
    pub status: SubStatus,
    pub memo: Option<String>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SubError {
    #[error("unknown subscription")]
    UnknownSubscription,
    #[error("merchant key mismatch")]
    MerchantKeyMismatch,
    #[error("amount mismatch expected {expected} got {got}")]
    AmountMismatch { expected: u64, got: u64 },
    #[error("script mismatch")]
    ScriptMismatch,
    #[error("overlapping period")]
    OverlappingPeriod,
}

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct ReceiptEpisode {
    invoices: BTreeMap<u64, Invoice>,
    subscriptions: BTreeMap<u64, Subscription>,
}

impl ReceiptEpisode {
    #[allow(dead_code)]
    pub fn invoices(&self) -> &BTreeMap<u64, Invoice> {
        &self.invoices
    }

    #[allow(dead_code)]
    pub fn invoice(&self, id: u64) -> Option<&Invoice> {
        self.invoices.get(&id)
    }
}

impl Episode for ReceiptEpisode {
    type Command = MerchantCommand;
    type CommandRollback = ();
    type CommandError = EpisodeError;

    fn initialize(_participants: Vec<PubKey>, _metadata: &PayloadMetadata) -> Self {
        Self { invoices: BTreeMap::new(), subscriptions: BTreeMap::new() }
    }

    fn execute(
        &mut self,
        cmd: &Self::Command,
        authorization: Option<PubKey>,
        metadata: &PayloadMetadata,
    ) -> Result<Self::CommandRollback, KdappEpisodeError<Self::CommandError>> {
        match cmd {
            MerchantCommand::CreateInvoice { invoice_id, amount, memo, guardian_keys } => {
                let merchant = authorization.ok_or(KdappEpisodeError::InvalidSignature)?;
                let inv = Invoice {
                    id: *invoice_id,
                    amount: *amount,
                    memo: memo.clone(),
                    status: InvoiceStatus::Open,
                    payer: None,
                    merchant_pubkey: merchant,
                    guardian_keys: guardian_keys.clone(),
                };
                self.invoices.insert(*invoice_id, inv);
            }
            MerchantCommand::MarkPaid { invoice_id, payer } => {
                if authorization != Some(*payer) {
                    return Err(KdappEpisodeError::InvalidSignature);
                }
                let inv = self.invoices.get_mut(invoice_id).ok_or(KdappEpisodeError::InvalidCommand(EpisodeError::UnknownInvoice))?;
                if inv.status != InvoiceStatus::Open {
                    return Err(KdappEpisodeError::InvalidCommand(EpisodeError::InvoiceNotOpen));
                }
                if let Some(existing_payer) = inv.payer {
                    if existing_payer != *payer {
                        return Err(KdappEpisodeError::InvalidCommand(EpisodeError::PayerMismatch));
                    }
                } else {
                    inv.payer = Some(*payer);
                }
                let outs = metadata.tx_outputs.as_ref().ok_or(KdappEpisodeError::InvalidCommand(EpisodeError::ScriptMismatch))?;
                let expected = p2pk_script(inv.merchant_pubkey);
                let mut matched = false;
                for o in outs {
                    if let Some(bytes) = &o.script_bytes {
                        if *bytes == expected {
                            if o.value < inv.amount {
                                return Err(KdappEpisodeError::InvalidCommand(EpisodeError::InsufficientPayment));
                            }
                            matched = true;
                            break;
                        }
                    }
                }
                if !matched {
                    return Err(KdappEpisodeError::InvalidCommand(EpisodeError::ScriptMismatch));
                }
                inv.status = InvoiceStatus::Paid;
            }
            MerchantCommand::AckReceipt { invoice_id } => {
                let inv = self.invoices.get_mut(invoice_id).ok_or(KdappEpisodeError::InvalidCommand(EpisodeError::UnknownInvoice))?;
                if inv.status != InvoiceStatus::Paid {
                    return Err(KdappEpisodeError::InvalidCommand(EpisodeError::InvoiceNotPaid));
                }
                inv.status = InvoiceStatus::Acked;
            }
            MerchantCommand::CancelInvoice { invoice_id } => {
                let inv = self.invoices.get_mut(invoice_id).ok_or(KdappEpisodeError::InvalidCommand(EpisodeError::UnknownInvoice))?;
                if inv.status != InvoiceStatus::Open {
                    return Err(KdappEpisodeError::InvalidCommand(EpisodeError::InvoiceNotOpen));
                }
                inv.status = InvoiceStatus::Canceled;
            }
            _ => {}
        }
        Ok(())
    }

    fn rollback(&mut self, _rollback: Self::CommandRollback) -> bool {
        true
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum EpisodeError {
    #[error("unknown invoice")]
    UnknownInvoice,
    #[error("invoice not open")]
    InvoiceNotOpen,
    #[error("invoice not paid")]
    InvoiceNotPaid,
    #[error("payer mismatch")]
    PayerMismatch,
    #[error("script mismatch")]
    ScriptMismatch,
    #[error("insufficient payment")]
    InsufficientPayment,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pki::p2pk_script;
    use kaspa_consensus_core::Hash;
    use kdapp::episode::{EpisodeError as KdappEpisodeError, PayloadMetadata, TxOutputInfo};
    use kdapp::pki::generate_keypair;

    fn md() -> PayloadMetadata {
        PayloadMetadata {
            accepting_hash: Hash::default(),
            accepting_daa: 0,
            accepting_time: 0,
            tx_id: Hash::default(),
            tx_outputs: None,
        }
    }

    #[test]
    fn invoice_receipt_and_ack_flow() {
        let (_skm, merchant_pk) = generate_keypair();
        let (_skp, payer_pk) = generate_keypair();
        let metadata = md();
        let mut ep = ReceiptEpisode::initialize(vec![merchant_pk], &metadata);

        // Receive invoice
        let cmd = MerchantCommand::CreateInvoice { invoice_id: 1, amount: 50, memo: Some("test".into()), guardian_keys: vec![] };
        ep.execute(&cmd, Some(merchant_pk), &metadata).expect("create");
        let inv = ep.invoice(1).expect("stored");
        assert_eq!(inv.amount, 50);
        assert!(matches!(inv.status, InvoiceStatus::Open));

        // Mark as paid
        let mut paid_md = md();
        let script = p2pk_script(merchant_pk);
        paid_md.tx_outputs = Some(vec![TxOutputInfo { value: 50, script_version: 0, script_bytes: Some(script) }]);
        let cmd = MerchantCommand::MarkPaid { invoice_id: 1, payer: payer_pk };
        ep.execute(&cmd, Some(payer_pk), &paid_md).expect("paid");
        let inv = ep.invoice(1).unwrap();
        assert!(matches!(inv.status, InvoiceStatus::Paid));
        assert_eq!(inv.payer, Some(payer_pk));

        // Acknowledge receipt
        let cmd = MerchantCommand::AckReceipt { invoice_id: 1 };
        ep.execute(&cmd, Some(merchant_pk), &metadata).expect("ack");
        let inv = ep.invoice(1).unwrap();
        assert!(matches!(inv.status, InvoiceStatus::Acked));
    }

    #[test]
    fn mark_paid_mismatched_amount() {
        let (_skm, merchant_pk) = generate_keypair();
        let (_skp, payer_pk) = generate_keypair();
        let metadata = md();
        let mut ep = ReceiptEpisode::initialize(vec![merchant_pk], &metadata);

        let cmd = MerchantCommand::CreateInvoice { invoice_id: 2, amount: 50, memo: None, guardian_keys: vec![] };
        ep.execute(&cmd, Some(merchant_pk), &metadata).expect("create");

        let mut paid_md = md();
        let script = p2pk_script(merchant_pk);
        paid_md.tx_outputs = Some(vec![TxOutputInfo { value: 40, script_version: 0, script_bytes: Some(script) }]);
        let cmd = MerchantCommand::MarkPaid { invoice_id: 2, payer: payer_pk };
        assert!(matches!(
            ep.execute(&cmd, Some(payer_pk), &paid_md),
            Err(KdappEpisodeError::InvalidCommand(EpisodeError::InsufficientPayment))
        ));
    }

    #[test]
    fn create_invoice_invalid_signature() {
        let (_sk, pk) = generate_keypair();
        let metadata = md();
        let mut ep = ReceiptEpisode::initialize(vec![pk], &metadata);

        let cmd = MerchantCommand::CreateInvoice { invoice_id: 3, amount: 10, memo: None, guardian_keys: vec![] };
        assert!(matches!(ep.execute(&cmd, None, &metadata), Err(KdappEpisodeError::InvalidSignature)));
    }

    #[test]
    fn mark_paid_unknown_payer() {
        let (_skm, merchant_pk) = generate_keypair();
        let (_sk1, payer1) = generate_keypair();
        let metadata = md();
        let mut ep = ReceiptEpisode::initialize(vec![merchant_pk], &metadata);

        let cmd = MerchantCommand::CreateInvoice { invoice_id: 4, amount: 25, memo: None, guardian_keys: vec![] };
        ep.execute(&cmd, Some(merchant_pk), &metadata).expect("create");

        let mut paid_md = md();
        let script = p2pk_script(merchant_pk);
        paid_md.tx_outputs = Some(vec![TxOutputInfo { value: 25, script_version: 0, script_bytes: Some(script) }]);
        let cmd = MerchantCommand::MarkPaid { invoice_id: 4, payer: payer1 };
        assert!(matches!(ep.execute(&cmd, Some(merchant_pk), &paid_md), Err(KdappEpisodeError::InvalidSignature)));
    }

    #[test]
    fn mark_paid_mismatched_script() {
        let (_skm, merchant_pk) = generate_keypair();
        let (_skp, payer_pk) = generate_keypair();
        let (_skx, other_pk) = generate_keypair();
        let metadata = md();
        let mut ep = ReceiptEpisode::initialize(vec![merchant_pk], &metadata);

        let cmd = MerchantCommand::CreateInvoice { invoice_id: 5, amount: 50, memo: None, guardian_keys: vec![] };
        ep.execute(&cmd, Some(merchant_pk), &metadata).expect("create");

        let mut paid_md = md();
        let script = p2pk_script(other_pk);
        paid_md.tx_outputs = Some(vec![TxOutputInfo { value: 50, script_version: 0, script_bytes: Some(script) }]);
        let cmd = MerchantCommand::MarkPaid { invoice_id: 5, payer: payer_pk };
        assert!(matches!(
            ep.execute(&cmd, Some(payer_pk), &paid_md),
            Err(KdappEpisodeError::InvalidCommand(EpisodeError::ScriptMismatch))
        ));
    }

    #[test]
    fn mark_paid_twice_fails() {
        let (_skm, merchant_pk) = generate_keypair();
        let (_skp, payer_pk) = generate_keypair();
        let metadata = md();
        let mut ep = ReceiptEpisode::initialize(vec![merchant_pk], &metadata);

        let create = MerchantCommand::CreateInvoice { invoice_id: 6, amount: 10, memo: None, guardian_keys: vec![] };
        ep.execute(&create, Some(merchant_pk), &metadata).unwrap();
        let mut paid_md = md();
        let script = p2pk_script(merchant_pk);
        paid_md.tx_outputs = Some(vec![TxOutputInfo { value: 10, script_version: 0, script_bytes: Some(script.clone()) }]);
        let pay = MerchantCommand::MarkPaid { invoice_id: 6, payer: payer_pk };
        ep.execute(&pay, Some(payer_pk), &paid_md).unwrap();
        let err = ep.execute(&pay, Some(payer_pk), &paid_md).unwrap_err();
        assert!(matches!(err, KdappEpisodeError::InvalidCommand(EpisodeError::InvoiceNotOpen)));
    }

    #[test]
    fn mark_paid_unknown_invoice_rejected() {
        let (_skm, merchant_pk) = generate_keypair();
        let (_skp, payer_pk) = generate_keypair();
        let metadata = md();
        let mut ep = ReceiptEpisode::initialize(vec![merchant_pk], &metadata);

        let mut paid_md = md();
        let script = p2pk_script(merchant_pk);
        paid_md.tx_outputs = Some(vec![TxOutputInfo { value: 10, script_version: 0, script_bytes: Some(script) }]);
        let cmd = MerchantCommand::MarkPaid { invoice_id: 999, payer: payer_pk };
        let err = ep.execute(&cmd, Some(payer_pk), &paid_md).unwrap_err();
        assert!(matches!(err, KdappEpisodeError::InvalidCommand(EpisodeError::UnknownInvoice)));
    }

    #[test]
    fn ack_requires_paid() {
        let (_skm, merchant_pk) = generate_keypair();
        let metadata = md();
        let mut ep = ReceiptEpisode::initialize(vec![merchant_pk], &metadata);

        let create = MerchantCommand::CreateInvoice { invoice_id: 7, amount: 10, memo: None, guardian_keys: vec![] };
        ep.execute(&create, Some(merchant_pk), &metadata).unwrap();
        let err = ep.execute(&MerchantCommand::AckReceipt { invoice_id: 7 }, Some(merchant_pk), &metadata).unwrap_err();
        assert!(matches!(err, KdappEpisodeError::InvalidCommand(EpisodeError::InvoiceNotPaid)));
    }
}
