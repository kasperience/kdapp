# Kaspa-Auth Daemon as a systemd User Service

This service runs the kaspa-auth daemon in your user session with a Unix socket in `XDG_RUNTIME_DIR` and data under `~/.local/share/kaspa-auth`.

## Install (user service)

- Build and install the binary: `cargo install --path . --bin kaspa-auth`
- Copy the unit to your user systemd folder: `mkdir -p ~/.config/systemd/user && cp systemd/kaspa-auth.service ~/.config/systemd/user/`
- Enable and start: `systemctl --user daemon-reload && systemctl --user enable --now kaspa-auth`
- Check logs: `journalctl --user -u kaspa-auth -f`

If your binary is not at `~/.cargo/bin/kaspa-auth`, edit `ExecStart=` accordingly (e.g., to your project `target/release/kaspa-auth`).

## Socket and paths

- Socket: `%t/kaspa-auth.sock` (usually `/run/user/$UID/kaspa-auth.sock`)
- Data dir: `~/.local/share/kaspa-auth`

## Verify

- Status: `cargo run --bin kaspa-auth -- daemon status --socket-path "$XDG_RUNTIME_DIR/kaspa-auth.sock"`
- Ping: `cargo run --bin kaspa-auth -- daemon send ping --socket-path "$XDG_RUNTIME_DIR/kaspa-auth.sock"`
- Create identity: `cargo run --bin kaspa-auth -- daemon send create --username alice --password secret`
- Unlock: `cargo run --bin kaspa-auth -- daemon send unlock --username alice --password secret`

## Uninstall

- `systemctl --user disable --now kaspa-auth`
- Remove unit file from `~/.config/systemd/user/kaspa-auth.service`

## Notes

- This runs in the user session (no root). Use a system-wide unit only if you know why you need it.
- The daemon uses the OS keychain via the `keyring` crate (Secret Service on Linux, etc.). In `--dev-mode` it writes to `~/.kaspa-auth/*.key`.
