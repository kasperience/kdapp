use std::fs;

use clap::Parser;
use env_logger::Env;
use kdapp_guardian::service::{run, Cli, GuardianConfig};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut cfg = GuardianConfig::default();
    if let Some(cfg_path) = &cli.config {
        let s = fs::read_to_string(cfg_path)?;
        cfg = toml::from_str(&s)?;
    }
    cfg = cli.merge_into_config(cfg);
    env_logger::Builder::from_env(Env::default().default_filter_or(&cfg.log_level)).init();
    let _handle = run(cfg);
    std::thread::park();
    Ok(())
}
