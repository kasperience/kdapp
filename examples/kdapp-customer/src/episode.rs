use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};
use kdapp::episode::{Episode, EpisodeError, PayloadMetadata};
use kdapp::pki::PubKey;
use thiserror::Error;

// Must mirror the wire shape used by kdapp-merchant to ensure Borsh compatibility
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum MerchantCommand {
    CreateInvoice { invoice_id: u64, amount: u64, memo: Option<String> },
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
}

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct ReceiptEpisode {
    invoices: BTreeMap<u64, Invoice>,
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
    type CommandError = CmdErr;

    fn initialize(_participants: Vec<PubKey>, _metadata: &PayloadMetadata) -> Self {
        Self { invoices: BTreeMap::new() }
    }

    fn execute(
        &mut self,
        cmd: &Self::Command,
        _authorization: Option<PubKey>,
        _metadata: &PayloadMetadata,
    ) -> Result<Self::CommandRollback, EpisodeError<Self::CommandError>> {
        match cmd {
            MerchantCommand::CreateInvoice { invoice_id, amount, memo } => {
                let inv = Invoice { id: *invoice_id, amount: *amount, memo: memo.clone(), status: InvoiceStatus::Open, payer: None };
                self.invoices.insert(*invoice_id, inv);
            }
            MerchantCommand::MarkPaid { invoice_id, payer } => {
                if let Some(inv) = self.invoices.get_mut(invoice_id) {
                    inv.status = InvoiceStatus::Paid;
                    inv.payer = Some(*payer);
                } else {
                    return Err(EpisodeError::InvalidCommand(CmdErr::Invalid));
                }
            }
            MerchantCommand::AckReceipt { invoice_id } => {
                if let Some(inv) = self.invoices.get_mut(invoice_id) {
                    inv.status = InvoiceStatus::Acked;
                } else {
                    return Err(EpisodeError::InvalidCommand(CmdErr::Invalid));
                }
            }
            MerchantCommand::CancelInvoice { invoice_id } => {
                if let Some(inv) = self.invoices.get_mut(invoice_id) {
                    inv.status = InvoiceStatus::Canceled;
                } else {
                    return Err(EpisodeError::InvalidCommand(CmdErr::Invalid));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn rollback(&mut self, _rollback: Self::CommandRollback) -> bool {
        true
    }
}

#[derive(Debug, Error, Clone)]
#[allow(dead_code)]
pub enum CmdErr {
    #[error("invalid command")]
    Invalid,
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::Hash;
    use kdapp::episode::PayloadMetadata;
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
        let (_sk, pk) = generate_keypair();
        let metadata = md();
        let mut ep = ReceiptEpisode::initialize(vec![pk], &metadata);

        // Receive invoice
        let cmd = MerchantCommand::CreateInvoice { invoice_id: 1, amount: 50, memo: Some("test".into()) };
        ep.execute(&cmd, Some(pk), &metadata).expect("create");
        let inv = ep.invoice(1).expect("stored");
        assert_eq!(inv.amount, 50);
        assert!(matches!(inv.status, InvoiceStatus::Open));

        // Mark as paid
        let cmd = MerchantCommand::MarkPaid { invoice_id: 1, payer: pk };
        ep.execute(&cmd, Some(pk), &metadata).expect("paid");
        let inv = ep.invoice(1).unwrap();
        assert!(matches!(inv.status, InvoiceStatus::Paid));
        assert_eq!(inv.payer, Some(pk));

        // Acknowledge receipt
        let cmd = MerchantCommand::AckReceipt { invoice_id: 1 };
        ep.execute(&cmd, Some(pk), &metadata).expect("ack");
        let inv = ep.invoice(1).unwrap();
        assert!(matches!(inv.status, InvoiceStatus::Acked));
    }
}
