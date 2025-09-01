use anyhow::Result;
use byteorder::{LittleEndian, WriteBytesExt};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        eprintln!("usage: submit-checkpoint <episode_id> <seq> <state_root_hex>");
        std::process::exit(2);
    }
    let episode_id: u64 = args.remove(0).parse()?;
    let seq: u64 = args.remove(0).parse()?;
    let root_hex = args.remove(0);
    let mut root = [0u8; 32];
    hex::decode_to_slice(&root_hex, &mut root)?;

    // OKCP v1 record
    let mut rec = Vec::with_capacity(4 + 1 + 8 + 8 + 32);
    rec.extend_from_slice(b"OKCP");
    rec.push(1u8);
    rec.write_u64::<LittleEndian>(episode_id)?;
    rec.write_u64::<LittleEndian>(seq)?;
    rec.extend_from_slice(&root);

    // In a full flow, wrap as kdapp payload with a dedicated PREFIX, then submit via generator.
    // For now, just print the hex so it can be fed to a submitter.
    println!("{}", hex::encode(rec));
    Ok(())
}
