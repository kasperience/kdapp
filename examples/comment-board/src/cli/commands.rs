use crate::cli::Args;
use crate::participant::run_participant;
use std::error::Error;

pub async fn handle_command(args: Args) -> Result<(), Box<dyn Error>> {
    // Initialize logger
    kaspa_core::log::init_logger(None, &args.log_level);

    // Run the comment board participant
    run_participant(args).await
}
