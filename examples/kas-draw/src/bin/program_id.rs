use anyhow::Result;
use blake2::{Blake2b512, Digest};
use std::io::{Read, Write};
use std::process::{Command, Stdio};

fn main() -> Result<()> {
    // Produce canonical bundle: git archive (tar), optionally gzip -n -9 if available
    let tar_bytes = {
        let output = Command::new("git").args(["archive", "--format=tar", "--prefix=ep/", "HEAD"]).stdout(Stdio::piped()).output()?;
        if !output.status.success() {
            eprintln!("git archive failed: status={}", output.status);
            std::process::exit(1);
        }
        output.stdout
    };

    // Try external gzip for determinism; fallback to raw tar bytes if gzip not found
    let bytes = match Command::new("gzip").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
        Ok(_) => {
            // gzip exists; compress with -n -9
            let mut child = Command::new("gzip").args(["-n", "-9"]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
            {
                let mut stdin = child.stdin.take().unwrap();
                stdin.write_all(&tar_bytes)?;
            }
            let mut out = Vec::new();
            child.stdout.take().unwrap().read_to_end(&mut out)?;
            out
        }
        Err(_) => {
            // gzip not present; use tar bytes directly
            eprintln!("warning: gzip not found; hashing tar bytes instead of tar.gz");
            tar_bytes
        }
    };

    // Compute BLAKE2b-512 and truncate to 32 bytes (BLAKE2b-256)
    let mut h = Blake2b512::new();
    h.update(&bytes);
    let id = h.finalize();
    let out = id.as_slice();
    println!("{}", hex::encode(&out[..32]));
    Ok(())
}
