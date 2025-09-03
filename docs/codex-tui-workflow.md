**Purpose**
- Document a safe, fast development loop combining `tmux` for layout/persistence with `codex-tui` for session safety and resume.

**Summary**
- Use `tmux` for splits, resilience, and continuous feedback.
- Use `codex-tui` to protect against accidental Ctrl+C and auto-resume state.
- Prevent parallel codex runs via the orchestrator’s lock and keep a single source of truth for commands.

**Why Tmux**
- **Layout:** Split panes for planner, editor, tests; keep context visible.
- **Resilience:** Sessions survive terminal disconnects; scrollback and copy-mode.
- **Feedback:** Watch tests in a pane for instant signal while iterating.

**Why codex-tui**
- **Ctrl+C Safety:** Intercepts Ctrl+C; quit requires Shift+Q pressed twice.
- **Auto-Resume:** Saves UI/config/log state to `~/.config/codex-tui/state.json`.
- **Consistency:** Central place to configure and launch codex workflows.

**Recommended Layout**
- Top-left: `codex-tui` (safe session manager)
- Bottom-left: editor (e.g., `nvim .` or `$EDITOR`)
- Right: test watcher (`entr` or `cargo-watch`) with continuous output

**Quick Start**
- Build: `cargo build -p codex-tui`
- Run in tmux via script: `bash scripts/launch_dev_tmux.sh`
- Manual tmux steps:
  - `tmux new -s dev`
  - `tmux split-window -h`
  - `tmux split-window -v -t 0`
  - Top-left: `cargo run -p codex-tui`
  - Bottom-left: `nvim .` (or your `$EDITOR`)
  - Right: `fd -e rs | entr -c sh -c 'cargo test -q || true'` or `cargo watch -q -x 'test -q'`

**Parallel Session Prevention**
- `orchestrator.py` uses `logs/.codex.lock` to avoid multiple concurrent codex runs.
- If lock is held, it auto-falls back to mock unless `--wait-for-lock` is set.

**Extra Safety Options**
- For raw codex CLI (outside TUI):
  - Temporarily disable Ctrl+C in that shell: `stty intr undef; codex ...; stty intr ^C`
  - Or ignore SIGINT: `bash -c 'trap "" INT; exec codex --model ...'`

**Keybindings (codex-tui)**
- **Shift+Q:** Press twice within 2s to quit.
- **Ctrl+C:** Protected by default; logs a tip instead of quitting.
- **Esc:** No-op; logs a tip.
- **n/p/Right/Left/Tab/BackTab:** Navigate steps.
- **Enter:** Apply selection (Model) or Save & Run (Review).
- **Space (Tools):** Toggle all tools.
- **c (Tools):** Toggle Ctrl+C protection.
- **e (Project):** Set project root to current working directory.

**Script**
- Use `scripts/launch_dev_tmux.sh` to auto-create the layout and run the right commands.
- Edit the script for custom session names or alternate watchers.

**Raw Codex (no TUI)**
- Use `scripts/launch_codex_tmux.sh` for a similar tmux layout with the original `codex` CLI.
- Top-left pane defines an alias `codexs` that runs `codex` with Ctrl+C protection:
  - Example: `codexs --model gpt-5-thinking "Plan next refactor steps for kdapp"`
- This uses `scripts/codex_safe.sh` to disable Ctrl+C during the codex run and re-enable it afterward.
