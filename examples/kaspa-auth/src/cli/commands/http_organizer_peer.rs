use clap::Args;
use secp256k1::Keypair;
use std::error::Error;
use crate::api::http::organizer_peer::run_http_peer;

#[derive(Args)]
pub struct HttpOrganizerPeerCommand {
    #[arg(short, long, default_value = "8080")]
    pub port: u16,
    
    #[arg(short, long)]
    pub key: Option<String>,
    
    // Storage options (set by CLI flags)
    #[arg(skip)]
    pub use_keychain: bool,
    
    #[arg(skip)]
    pub dev_mode: bool,
}

impl HttpOrganizerPeerCommand {
    pub fn set_storage_options(&mut self, use_keychain: bool, dev_mode: bool) {
        self.use_keychain = use_keychain;
        self.dev_mode = dev_mode;
    }
    
    pub async fn execute(self) -> Result<(), Box<dyn Error>> {
        let provided_private_key = self.key.as_deref();
        
        if self.use_keychain {
            println!("🔐 HTTP organizer peer will use OS keychain for wallet storage");
        }
        
        // TODO: Pass keychain options to run_http_peer function
        run_http_peer(provided_private_key, self.port).await
    }
}



