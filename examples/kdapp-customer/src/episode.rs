use borsh::{BorshDeserialize, BorshSerialize};
use kdapp::episode::{Episode, EpisodeError, PayloadMetadata};
use kdapp::pki::PubKey;

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum MerchantCommand {
    MarkPaid { invoice_id: u64, payer: PubKey },
    AckReceipt { invoice_id: u64 },
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct ReceiptEpisode;

impl Episode for ReceiptEpisode {
    type Command = MerchantCommand;
    type CommandRollback = (); 
    type CommandError = (); 

    fn initialize(_participants: Vec<PubKey>, _metadata: &PayloadMetadata) -> Self {
        Self
    }

    fn execute(
        &mut self,
        _cmd: &Self::Command,
        _authorization: Option<PubKey>,
        _metadata: &PayloadMetadata,
    ) -> Result<Self::CommandRollback, EpisodeError<Self::CommandError>> {
        Ok(())
    }

    fn rollback(&mut self, _rollback: Self::CommandRollback) -> bool {
        true
    }
}
