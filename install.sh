#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
APP_DIR="$HOME/Library/Application Support/MX4Haptic"
BIN_DIR="$APP_DIR/bin"
BIN="$BIN_DIR/mx4-haptic-daemon"
PLIST="$HOME/Library/LaunchAgents/com.local.mx4-haptic.plist"
SOURCE_BIN="$SCRIPT_DIR/target/release/mx4-haptic-daemon"
UID_VALUE=$(id -u)

if [ ! -x "$SOURCE_BIN" ]; then
  if ! command -v cargo >/dev/null 2>&1; then
    echo "Missing $SOURCE_BIN. Build first with: cargo build --release" >&2
    exit 1
  fi
  (cd "$SCRIPT_DIR" && cargo build --release)
fi

mkdir -p "$BIN_DIR" "$HOME/Library/LaunchAgents"
install -m 755 "$SOURCE_BIN" "$BIN"
sed \
  -e "s|__MX4_HAPTIC_DAEMON_PATH__|$BIN|g" \
  -e "s|__MX4_HAPTIC_LOG_PATH__|$APP_DIR/daemon.log|g" \
  "$SCRIPT_DIR/com.local.mx4-haptic.plist" > "$PLIST"

launchctl bootout "gui/$UID_VALUE" "$PLIST" >/dev/null 2>&1 || true
launchctl bootstrap "gui/$UID_VALUE" "$PLIST"
launchctl kickstart -k "gui/$UID_VALUE/com.local.mx4-haptic"

echo "Installed $BIN"
echo "Grant Accessibility permission to $BIN in System Settings before using it."
