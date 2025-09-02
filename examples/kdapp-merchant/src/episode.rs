#![allow(clippy::enum_variant_names)]
use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};
use kdapp::episode::{Episode, EpisodeError, PayloadMetadata};
use kdapp::pki::PubKey;
use crate::storage;

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum MerchantCommand {
    CreateInvoice { invoice_id: u64, amount: u64, memo: Option<String> },
    MarkPaid { invoice_id: u64, payer: Option<PubKey> },
    AckReceipt { invoice_id: u64 },
    CancelInvoice { invoice_id: u64 },
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum MerchantRollback {
    UndoCreate { invoice_id: u64 },
    UndoPaid { invoice_id: u64 },
    UndoAck { invoice_id: u64 },
    UndoCancel { invoice_id: u64 },
}

#[derive(thiserror::Error, Debug)]
pub enum MerchantError {
    #[error("invoice exists")]
    Exists,
    #[error("invoice not found")]
    NotFound,
    #[error("invalid amount")]
    InvalidAmount,
    #[error("already paid")]
    AlreadyPaid,
    #[error("already acked")]
    AlreadyAcked,
    #[error("already canceled")]
    AlreadyCanceled,
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
    pub created_at: u64,
    pub last_update: u64,
    pub status: InvoiceStatus,
    pub payer: Option<PubKey>,
    pub carrier_tx: Option<kaspa_consensus_core::Hash>,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct ReceiptEpisode {
    pub merchant_keys: Vec<PubKey>,
    pub invoices: BTreeMap<u64, Invoice>,
}

impl Episode for ReceiptEpisode {
    type Command = MerchantCommand;
    type CommandRollback = MerchantRollback;
    type CommandError = MerchantError;

    fn initialize(participants: Vec<PubKey>, _metadata: &PayloadMetadata) -> Self {
        let invoices = storage::load_invoices();
        Self { merchant_keys: participants, invoices }
    }

    fn execute(
        &mut self,
        cmd: &Self::Command,
        authorization: Option<PubKey>,
        metadata: &PayloadMetadata,
    ) -> Result<Self::CommandRollback, EpisodeError<Self::CommandError>> {
        match cmd {
            MerchantCommand::CreateInvoice { invoice_id, amount, memo } => {
                if self.invoices.contains_key(invoice_id) {
                    return Err(EpisodeError::InvalidCommand(MerchantError::Exists));
                }
                if *amount == 0 {
                    return Err(EpisodeError::InvalidCommand(MerchantError::InvalidAmount));
                }
                // Require merchant auth for creating invoices
                if let Some(pk) = authorization {
                    if !self.merchant_keys.contains(&pk) {
                        return Err(EpisodeError::Unauthorized);
                    }
                } else {
                    return Err(EpisodeError::Unauthorized);
                }
                let created = Invoice {
                    id: *invoice_id,
                    amount: *amount,
                    memo: memo.clone(),
                    created_at: metadata.accepting_time,
                    last_update: metadata.accepting_time,
                    status: InvoiceStatus::Open,
                    payer: None,
                    carrier_tx: None,
                };
                self.invoices.insert(*invoice_id, created.clone());
                storage::put_invoice(&created);
                Ok(MerchantRollback::UndoCreate { invoice_id: *invoice_id })
            }
            MerchantCommand::MarkPaid { invoice_id, payer } => {
                let inv = self
                    .invoices
                    .get_mut(invoice_id)
                    .ok_or(EpisodeError::InvalidCommand(MerchantError::NotFound))?;
                if inv.status == InvoiceStatus::Canceled {
                    return Err(EpisodeError::InvalidCommand(MerchantError::AlreadyCanceled));
                }
                if inv.status == InvoiceStatus::Acked || inv.status == InvoiceStatus::Paid {
                    return Err(EpisodeError::InvalidCommand(MerchantError::AlreadyPaid));
                }
                // If proxy provided tx summary, require at least one output is >= invoice amount (coarse check)
                if let Some(outs) = &metadata.tx_outputs {
                    let ok = outs.iter().any(|o| o.value >= inv.amount);
                    if !ok {
                        return Err(EpisodeError::InvalidCommand(MerchantError::InvalidAmount));
                    }
                }
                inv.status = InvoiceStatus::Paid;
                inv.last_update = metadata.accepting_time;
                inv.payer = *payer;
                inv.carrier_tx = Some(metadata.tx_id);
                storage::put_invoice(inv);
                Ok(MerchantRollback::UndoPaid { invoice_id: *invoice_id })
            }
            MerchantCommand::AckReceipt { invoice_id } => {
                let inv = self
                    .invoices
                    .get_mut(invoice_id)
                    .ok_or(EpisodeError::InvalidCommand(MerchantError::NotFound))?;
                if inv.status == InvoiceStatus::Canceled {
                    return Err(EpisodeError::InvalidCommand(MerchantError::AlreadyCanceled));
                }
                if inv.status == InvoiceStatus::Acked {
                    return Err(EpisodeError::InvalidCommand(MerchantError::AlreadyAcked));
                }
                if inv.status != InvoiceStatus::Paid {
                    return Err(EpisodeError::InvalidCommand(MerchantError::NotFound));
                }
                inv.status = InvoiceStatus::Acked;
                inv.last_update = metadata.accepting_time;
                storage::put_invoice(inv);
                Ok(MerchantRollback::UndoAck { invoice_id: *invoice_id })
            }
            MerchantCommand::CancelInvoice { invoice_id } => {
                let inv = self
                    .invoices
                    .get_mut(invoice_id)
                    .ok_or(EpisodeError::InvalidCommand(MerchantError::NotFound))?;
                if inv.status == InvoiceStatus::Canceled {
                    return Err(EpisodeError::InvalidCommand(MerchantError::AlreadyCanceled));
                }
                if matches!(inv.status, InvoiceStatus::Paid | InvoiceStatus::Acked) {
                    return Err(EpisodeError::InvalidCommand(MerchantError::AlreadyPaid));
                }
                inv.status = InvoiceStatus::Canceled;
                inv.last_update = metadata.accepting_time;
                storage::put_invoice(inv);
                Ok(MerchantRollback::UndoCancel { invoice_id: *invoice_id })
            }
        }
    }

    fn rollback(&mut self, rollback: Self::CommandRollback) -> bool {
        match rollback {
            MerchantRollback::UndoCreate { invoice_id } => {
                storage::delete_invoice(invoice_id);
                self.invoices.remove(&invoice_id).is_some()
            }
            MerchantRollback::UndoPaid { invoice_id } => {
                if let Some(inv) = self.invoices.get_mut(&invoice_id) {
                    inv.status = InvoiceStatus::Open;
                    inv.payer = None;
                    inv.carrier_tx = None;
                    storage::put_invoice(inv);
                    true
                } else {
                    false
                }
            }
            MerchantRollback::UndoAck { invoice_id } => {
                if let Some(inv) = self.invoices.get_mut(&invoice_id) {
                    inv.status = InvoiceStatus::Paid;
                    storage::put_invoice(inv);
                    true
                } else {
                    false
                }
            }
            MerchantRollback::UndoCancel { invoice_id } => {
                if let Some(inv) = self.invoices.get_mut(&invoice_id) {
                    inv.status = InvoiceStatus::Open;
                    storage::put_invoice(inv);
                    true
                } else {
                    false
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::Hash;
    use kdapp::episode::{PayloadMetadata, TxOutputInfo};
    use kdapp::pki::generate_keypair;

    fn md() -> PayloadMetadata {
        PayloadMetadata {
            accepting_hash: Hash::default(),
            accepting_daa: 0,
            accepting_time: 1,
            tx_id: Hash::default(),
            tx_outputs: None,
        }
    }

    #[test]
    fn create_invoice_requires_auth() {
        let (_sk, pk) = generate_keypair();
        let metadata = md();
        let mut ep = ReceiptEpisode::initialize(vec![pk], &metadata);
        let cmd = MerchantCommand::CreateInvoice { invoice_id: 1, amount: 10, memo: None };
        let err = ep.execute(&cmd, None, &metadata).unwrap_err();
        match err {
            EpisodeError::Unauthorized => {}
            _ => panic!("expected Unauthorized"),
        }
    }

    #[test]
    fn pay_and_ack_happy_path() {
        let (_sk, pk) = generate_keypair();
        let metadata = md();
        let mut ep = ReceiptEpisode::initialize(vec![pk], &metadata);
        // Create
        let cmd = MerchantCommand::CreateInvoice { invoice_id: 1, amount: 100, memo: Some("x".into()) };
        let _rb = ep.execute(&cmd, Some(pk), &metadata).expect("create ok");
        // Pay (coarse check with outputs >= amount)
        let mut md_paid = metadata.clone();
        md_paid.tx_outputs = Some(vec![TxOutputInfo { value: 100, script_version: 0, script_bytes: None }]);
        let cmd = MerchantCommand::MarkPaid { invoice_id: 1, payer: Some(pk) };
        let _rb = ep.execute(&cmd, None, &md_paid).expect("pay ok");
        assert!(matches!(ep.invoices.get(&1).unwrap().status, InvoiceStatus::Paid));
        // Ack
        let cmd = MerchantCommand::AckReceipt { invoice_id: 1 };
        let _rb = ep.execute(&cmd, Some(pk), &metadata).expect("ack ok");
        assert!(matches!(ep.invoices.get(&1).unwrap().status, InvoiceStatus::Acked));
    }
}
