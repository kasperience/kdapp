#!/usr/bin/env bash
set -euo pipefail

# Install an autostart .desktop entry that runs the first-login wizard once.

WIZARD="$HOME/.local/share/omarchy/bin/kaspa-first-login-wizard.sh"
SRC_WIZARD_REPO="$(cd "$(dirname "$0")" && pwd)/kaspa-first-login-wizard.sh"
AUTOSTART_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"
DESKTOP_FILE="$AUTOSTART_DIR/kaspa-first-login-wizard.desktop"

mkdir -p "$AUTOSTART_DIR"
mkdir -p "$(dirname "$WIZARD")"

cp -f "$SRC_WIZARD_REPO" "$WIZARD"
chmod +x "$WIZARD"

cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Type=Application
Name=Kaspa First Login Wizard
Comment=Initialize Kaspa Auth identity and show wallet address
Exec=$WIZARD
Terminal=true
X-GNOME-Autostart-enabled=true
X-KDE-StartupNotify=false
OnlyShowIn=GNOME;KDE;LXQt;XFCE;Hyprland;
EOF

echo "Installed autostart: $DESKTOP_FILE"
echo "Wizard script: $WIZARD"
echo "Disable after first run by removing the marker: ~/.local/share/kaspa-auth/.first_login_done"

