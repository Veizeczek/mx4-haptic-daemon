#!/bin/sh
set -eu

PLIST="$HOME/Library/LaunchAgents/com.local.mx4-haptic.plist"
UID_VALUE=$(id -u)

if [ -f "$PLIST" ]; then
  launchctl bootout "gui/$UID_VALUE" "$PLIST" >/dev/null 2>&1 || true
  rm "$PLIST"
fi

echo "Removed the MX4 Haptic LaunchAgent. The binary and config were left in Application Support."
