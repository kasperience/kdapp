use clap::Parser;
use std::error::Error;

mod cli;
mod comments;
mod episode;
mod participant;
mod utils;
mod wallet;

use cli::{Args, handle_command};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    handle_command(args).await
}