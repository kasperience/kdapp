use anyhow::Result;
use blake2::{Blake2b256, Digest};
use std::io::Read;
use std::process::{Command, Stdio};

fn main() -> Result<()> {
    // Produce canonical bundle: git archive | gzip -n -9
    let mut git = Command::new("git")
        .args(["archive", "--format=tar", "--prefix=ep/", "HEAD"])
        .stdout(Stdio::piped())
        .spawn()?;
    let mut gz = Command::new("gzip")
        .args(["-n", "-9"]) // no timestamps, max compression (determinism)
        .stdin(git.stdout.take().unwrap())
        .stdout(Stdio::piped())
        .spawn()?;

    let mut bytes = Vec::new();
    gz.stdout.take().unwrap().read_to_end(&mut bytes)?;

    let mut h = Blake2b256::new();
    h.update(&bytes);
    let id = h.finalize();
    println!("{}", hex::encode(id));
    Ok(())
}

