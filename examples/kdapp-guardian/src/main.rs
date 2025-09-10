use env_logger::Env;
use kdapp_guardian::service::{run, GuardianConfig};

fn main() {
    let config = GuardianConfig::from_args();
    env_logger::Builder::from_env(Env::default().default_filter_or(&config.log_level)).init();
    let _handle = run(&config);
    std::thread::park();
}
