PS C:\Users\mariu\Documents\kdapp\kdapp\examples\comment-it> cargo check
warning: C:\Users\mariu\Documents\kdapp\kdapp\examples\kdapp-wallet\Cargo.toml: only one of `license` or `license-file` is necessary
`license` should be used if the package license can be expressed with a standard SPDX expression.
`license-file` should be used if the package uses a non-standard license.
See https://doc.rust-lang.org/cargo/reference/manifest.html#the-license-and-license-file-fields for more information.
warning: type `PeerStats` is more private than the item `ResilientPeerConnection::get_peer_stats`
   --> examples\comment-it\src\cli\resilient_peer_connection.rs:186:5
    |
186 |     pub fn get_peer_stats(&self) -> &std::collections::HashMap<String, PeerStats> {
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ method `ResilientPeerConnection::get_peer_stats` is reachable at visibility `pub`
    |
note: but type `PeerStats` is only usable at visibility `pub(self)`
   --> examples\comment-it\src\cli\resilient_peer_connection.rs:16:1
    |
16  | struct PeerStats {
    | ^^^^^^^^^^^^^^^^
    = note: `#[warn(private_interfaces)]` on by default

warning: field `needs_data` is never read
  --> examples\comment-it\src\cli\commands\test_api.rs:23:5
   |
19 | struct ApiEndpoint {
   |        ----------- field in this struct
...
23 |     needs_data: bool,
   |     ^^^^^^^^^^
   |
   = note: `ApiEndpoint` has a derived impl for the trait `Debug`, but this is intentionally ignored during dead code analysis
   = note: `#[warn(dead_code)]` on by default

warning: methods `success_rate` and `is_healthy` are never used
   --> examples\comment-it\src\cli\resilient_peer_connection.rs:282:12
    |
281 | impl PeerStats {
    | -------------- methods in this implementation
282 |     pub fn success_rate(&self) -> f64 {
    |            ^^^^^^^^^^^^
...
291 |     pub fn is_healthy(&self) -> bool {
    |            ^^^^^^^^^^

warning: `comment-it` (lib) generated 3 warnings
    Checking comment-it v0.1.0 (C:\Users\mariu\Documents\kdapp\kdapp\examples\comment-it)
warning: unused import: `error`
  --> examples\comment-it\src\main.rs:13:11
   |
13 | use log::{error, info, warn};
   |           ^^^^^
   |
   = note: `#[warn(unused_imports)]` on by default

error[E0063]: missing field `session_token` in initializer of `UnifiedCommand`
   --> examples\comment-it\src\main.rs:126:35
    |
126 |                     let command = comment_it::core::UnifiedCommand::SubmitComment { text: text.clone() };
    |                                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ missing `session_token`

error[E0063]: missing field `session_token` in initializer of `UnifiedCommand`
   --> examples\comment-it\src\main.rs:231:35
    |
231 |                     let command = comment_it::core::UnifiedCommand::RevokeSession { signature };
    |                                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ missing `session_token`

error[E0277]: the trait bound `secp256k1::PublicKey: From<secp256k1::XOnlyPublicKey>` is not satisfied
   --> examples\comment-it\src\main.rs:232:82
    |
232 |                     let public_key = PubKey(signer_keypair.x_only_public_key().0.into());
    |                                                                                  ^^^^ the trait `From<secp256k1::XOnlyPublicKey>` is not implemented for `secp256k1::PublicKey`
    |
    = help: the following other types implement trait `From<T>`:
              `secp256k1::PublicKey` implements `From<&secp256k1::Keypair>`
              `secp256k1::PublicKey` implements `From<secp256k1::Keypair>`
              `secp256k1::PublicKey` implements `From<secp256k1::secp256k1_sys::PublicKey>`
    = note: required for `secp256k1::XOnlyPublicKey` to implement `Into<secp256k1::PublicKey>`

error[E0308]: mismatched types
   --> examples\comment-it\src\main.rs:234:60
    |
234 | ...   EpisodeMessage::new_signed_command(episode_id, command, signer_keypair.secret_key(), public_key);
    |       ---------------------------------- ^^^^^^^^^^ expected `u32`, found `u64`
    |       |
    |       arguments to this function are incorrect
    |
note: associated function defined here
   --> C:\Users\mariu\Documents\kdapp\kdapp\kdapp\src\engine.rs:65:12
    |
65  |     pub fn new_signed_command(episode_id: EpisodeId, cmd: G::Command, sk: SecretKey, pk: PubKey) -> Self {
    |            ^^^^^^^^^^^^^^^^^^
help: you can convert a `u64` to a `u32` and panic if the converted value doesn't fit
    |
234 |                         EpisodeMessage::new_signed_command(episode_id.try_into().unwrap(), command, signer_keypair.secret_key(), public_key);
    |                                                                      ++++++++++++++++++++

error[E0609]: no field `kaspa_address` on type `KaspaAuthWallet`
  --> examples\comment-it\src\main.rs:40:40
   |
40 |     let signer_address = signer_wallet.kaspa_address;
   |                                        ^^^^^^^^^^^^^ unknown field
   |
   = note: available fields are: `keypair`, `config`, `was_created`

error[E0061]: this function takes 1 argument but 2 arguments were supplied
   --> examples\comment-it\src\main.rs:54:31
    |
54  |     tokio::spawn(async move { run_auth_server(auth_config, event_tx).await });
    |                               ^^^^^^^^^^^^^^^              -------- unexpected argument #2 of type `tokio::sync::mpsc::Sender<_>`
    |
note: function defined here
   --> C:\Users\mariu\Documents\kdapp\kdapp\examples\comment-it\src\episode_runner.rs:246:14
    |
246 | pub async fn run_auth_server(config: AuthServerConfig) -> Result<(), Box<dyn std::error::Error>> {
    |              ^^^^^^^^^^^^^^^
help: remove the extra argument
    |
54  -     tokio::spawn(async move { run_auth_server(auth_config, event_tx).await });
54  +     tokio::spawn(async move { run_auth_server(auth_config).await });
    |

error[E0277]: `dyn StdError` cannot be sent between threads safely
   --> examples\comment-it\src\main.rs:54:18
    |
54  |     tokio::spawn(async move { run_auth_server(auth_config, event_tx).await });
    |     ------------ ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `dyn StdError` cannot be sent between threads safely
    |     |
    |     required by a bound introduced by this call
    |
    = help: the trait `std::marker::Send` is not implemented for `dyn StdError`
    = note: required for `Unique<dyn StdError>` to implement `std::marker::Send`
note: required because it appears within the type `Box<dyn StdError>`
   --> C:\Users\mariu\.rustup\toolchains\stable-x86_64-pc-windows-msvc\lib/rustlib/src/rust\library\alloc\src\boxed.rs:231:12
    |
231 | pub struct Box<
    |            ^^^
note: required because it appears within the type `Result<(), Box<dyn StdError>>`
   --> C:\Users\mariu\.rustup\toolchains\stable-x86_64-pc-windows-msvc\lib/rustlib/src/rust\library\core\src\result.rs:548:10
    |
548 | pub enum Result<T, E> {
    |          ^^^^^^
note: required by a bound in `tokio::spawn`
   --> C:\Users\mariu\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\tokio-1.45.1\src\task\spawn.rs:169:20
    |
166 |     pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
    |            ----- required by a bound in this function
...
169 |         F::Output: Send + 'static,
    |                    ^^^^ required by this bound in `spawn`

error[E0599]: no method named `clone` found for struct `TransactionGenerator` in the current scope
  --> examples\comment-it\src\main.rs:73:26
   |
73 |             tx_generator.clone(),
   |                          ^^^^^ method not found in `TransactionGenerator`

error[E0277]: `(dyn StdError + 'static)` cannot be sent between threads safely
   --> examples\comment-it\src\main.rs:70:22
    |
70  |           tokio::spawn(handle_connection(
    |  _________------------_^
    | |         |
    | |         required by a bound introduced by this call
71  | |             stream,
72  | |             signer_keypair.clone(),
73  | |             tx_generator.clone(),
...   |
76  | |             peer_event_rx,
77  | |         ));
    | |_________^ `(dyn StdError + 'static)` cannot be sent between threads safely
    |
    = help: the trait `std::marker::Send` is not implemented for `(dyn StdError + 'static)`
    = note: required for `Unique<(dyn StdError + 'static)>` to implement `std::marker::Send`
note: required because it appears within the type `Box<(dyn StdError + 'static)>`
   --> C:\Users\mariu\.rustup\toolchains\stable-x86_64-pc-windows-msvc\lib/rustlib/src/rust\library\alloc\src\boxed.rs:231:12
    |
231 | pub struct Box<
    |            ^^^
note: required because it appears within the type `Result<(), Box<(dyn StdError + 'static)>>`
   --> C:\Users\mariu\.rustup\toolchains\stable-x86_64-pc-windows-msvc\lib/rustlib/src/rust\library\core\src\result.rs:548:10
    |
548 | pub enum Result<T, E> {
    |          ^^^^^^
note: required by a bound in `tokio::spawn`
   --> C:\Users\mariu\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\tokio-1.45.1\src\task\spawn.rs:169:20
    |
166 |     pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
    |            ----- required by a bound in this function
...
169 |         F::Output: Send + 'static,
    |                    ^^^^ required by this bound in `spawn`

Some errors have detailed explanations: E0061, E0063, E0277, E0308, E0599, E0609.
For more information about an error, try `rustc --explain E0061`.
warning: `comment-it` (bin "comment-it") generated 1 warning
error: could not compile `comment-it` (bin "comment-it") due to 9 previous errors; 1 warning emitted