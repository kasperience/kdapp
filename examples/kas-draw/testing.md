Off‑Chain ACK + Close — Quick Test

- Start engine + router:
  - `cargo run -p kas-draw -- offchain-engine`
  - Or use the script which can also skip engine start:
    - `examples/kas-draw/offchain_demo.ps1 -NoStartEngine` (engine must be running already)

- Reset local seq (if needed):
  - Delete `target/kas_draw_offchain_seq.txt`

- Happy path (episode 10):
  - Or run the script:
    - `examples/kas-draw/offchain_demo.ps1 -EpisodeId 10 -Key <hex>`
    - If engine already running: add `-NoStartEngine`
  - New (seq 0, include participant):
    - `cargo run -p kas-draw -- offchain-send --type new --episode-id 10 --kaspa-private-key <hex>`
  - Buy (seq 1, signed):
    - `cargo run -p kas-draw -- offchain-send --type cmd --episode-id 10 --kaspa-private-key <hex> --amount 100000000 1 2 3 4 5`
  - Draw (seq 2) after ~15s:
    - `cargo run -p kas-draw -- offchain-send --type cmd --episode-id 10 --entropy demo`
  - Close (seq 3):
    - `cargo run -p kas-draw -- offchain-send --type close --episode-id 10`

- Expected:
  - Sender prints: `ack received for ep 10 seq 0/1/2/3`
  - Engine dashboard shows BUY/DRAW/CLOSE lines; tower logs `Finalized` on close.

- Strict sequencing:
  - Router requires exact increments and `New` at seq 0.
  - If engine/router restarts, re‑send `New` (seq 0) or reset the seq file(s) and start over.
  - The seq file path is relative to your current directory. Common locations:
    - Root: `target/kas_draw_offchain_seq.txt`
    - Example dir: `examples/kas-draw/target/kas_draw_offchain_seq.txt`
    - Use one consistent working directory, or delete both files when recovering.

- Flags:
  - Engine: `--no-ack`, `--no-close` to disable features.
  - Sender: `--no-ack`, `--force-seq <n>` to override sequence when recovering.
