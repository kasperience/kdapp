use kdapp_guardian::service::{run, GuardianConfig};

fn main() {
    env_logger::init();
    let config = GuardianConfig::from_args();
    let _handle = run(&config);
    std::thread::park();
}
