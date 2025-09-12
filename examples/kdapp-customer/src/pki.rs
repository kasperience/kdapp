use kdapp::pki::PubKey;

pub fn p2pk_script(pubkey: PubKey) -> Vec<u8> {
    let mut script = Vec::with_capacity(35);
    script.push(33);
    script.extend_from_slice(&pubkey.0.serialize());
    script.push(0xac);
    script
}
