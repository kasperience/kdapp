use kdapp_wallet::cli;

fn main() {
    if let Err(e) = cli::main() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}