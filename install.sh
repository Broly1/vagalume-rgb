#!/bin/sh
# Builds and installs for the current user. Add --restore to also re-apply the
# saved lighting at login.
set -eu
cd "$(dirname "$0")"

cargo build --release
data="${XDG_DATA_HOME:-$HOME/.local/share}"
install -Dm755 target/release/vagalume-rgb "$HOME/.local/bin/vagalume-rgb"
install -Dm644 packaging/org.vagalumergb.VagalumeRgb.desktop \
    "$data/applications/org.vagalumergb.VagalumeRgb.desktop"
install -Dm644 packaging/icons/hicolor/scalable/apps/org.vagalumergb.VagalumeRgb.svg \
    "$data/icons/hicolor/scalable/apps/org.vagalumergb.VagalumeRgb.svg"

if [ "${1:-}" = "--restore" ]; then
    install -Dm644 packaging/vagalume-rgb-restore.service \
        "${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/vagalume-rgb-restore.service"
    systemctl --user daemon-reload
    systemctl --user enable vagalume-rgb-restore.service
fi
