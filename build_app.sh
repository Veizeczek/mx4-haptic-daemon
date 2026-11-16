#!/bin/bash
set -e

APP_NAME="MX4 Haptic"
APP_DIR="$APP_NAME.app"
BIN_NAME="mx4-haptic-daemon"

echo "Building release binary..."
cargo build --release

echo "Creating App Bundle structure..."
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Resources"

echo "Copying binary..."
cp "target/release/$BIN_NAME" "$APP_DIR/Contents/MacOS/"

echo "Creating Info.plist..."
cat << PLIST > "$APP_DIR/Contents/Info.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>$BIN_NAME</string>
    <key>CFBundleIdentifier</key>
    <string>com.local.mx4-haptic</string>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.15</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST

echo "Signing the App Bundle..."
codesign --force --deep --sign - "$APP_DIR"

echo "App Bundle created at $APP_DIR"
