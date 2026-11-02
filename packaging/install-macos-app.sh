#!/usr/bin/env bash
# ==============================================================================
# Installer for Typora Rust Reader on Apple Silicon macOS
# Creates /Applications/Typora Rust Reader.app and CLI symlinks
# ==============================================================================
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ "$(basename "$SCRIPT_DIR")" = "packaging" ]; then
  PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
else
  PROJECT_ROOT="$SCRIPT_DIR"
fi

echo "=================================================="
echo " Installing Typora Rust Reader on macOS"
echo "=================================================="

# 1. Check or compile ARM64 release binary
TARGET_BIN="$PROJECT_ROOT/target/release/typora-rust-reader"
if [ ! -f "$TARGET_BIN" ]; then
  echo "Release binary not found at $TARGET_BIN."
  echo "Compiling native Apple Silicon release binary..."
  (cd "$PROJECT_ROOT" && cargo build --release)
fi

if [ ! -f "$TARGET_BIN" ]; then
  echo "❌ Error: Could not build $TARGET_BIN" >&2
  exit 1
fi
echo "✔ Found binary: $TARGET_BIN"

# 2. Determine Applications destination
APP_NAME="Typora Rust Reader.app"
if [ -w "/Applications" ]; then
  APP_DEST="/Applications/$APP_NAME"
else
  APP_DEST="$HOME/Applications/$APP_NAME"
  mkdir -p "$HOME/Applications"
fi

echo "✔ Installing app bundle to: $APP_DEST"

# 3. Create macOS .app directory structure
rm -rf "$APP_DEST"
mkdir -p "$APP_DEST/Contents/MacOS"
mkdir -p "$APP_DEST/Contents/Resources"

# 4. Copy binary
cp "$TARGET_BIN" "$APP_DEST/Contents/MacOS/Typora Rust Reader"
chmod 755 "$APP_DEST/Contents/MacOS/Typora Rust Reader"

# 5. Write comprehensive Info.plist with Markdown UTI and file associations
cat << 'EOF' > "$APP_DEST/Contents/Info.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>Typora Rust Reader</string>
    <key>CFBundleIdentifier</key>
    <string>com.typora.rust-reader</string>
    <key>CFBundleName</key>
    <string>Typora Rust Reader</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key>
            <string>Markdown Document</string>
            <key>CFBundleTypeRole</key>
            <string>Editor</string>
            <key>LSHandlerRank</key>
            <string>Owner</string>
            <key>LSItemContentTypes</key>
            <array>
                <string>net.daringfireball.markdown</string>
                <string>public.plain-text</string>
                <string>public.text</string>
            </array>
            <key>CFBundleTypeExtensions</key>
            <array>
                <string>md</string>
                <string>markdown</string>
                <string>mdown</string>
                <string>txt</string>
            </array>
        </dict>
    </array>
</dict>
</plist>
EOF

# 6. Install CLI tools to ~/.local/bin
mkdir -p "$HOME/.local/bin"
cp -f "$TARGET_BIN" "$HOME/.local/bin/typora-rust-reader"
chmod 755 "$HOME/.local/bin/typora-rust-reader"
ln -sf "$HOME/.local/bin/typora-rust-reader" "$HOME/.local/bin/typora-reader"
echo "✔ Installed CLI commands: ~/.local/bin/typora-rust-reader and ~/.local/bin/typora-reader"

# 7. Remove quarantine attributes & set permissions
chmod -R 755 "$APP_DEST"
xattr -cr "$APP_DEST" 2>/dev/null || true

# 8. Register with LaunchServices
if [ -f "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister" ]; then
  /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f -R "$APP_DEST"
fi

# 9. Reload Finder
killall Finder 2>/dev/null || true

echo ""
echo "=================================================="
echo " 🎉 Installation Complete!"
echo " App installed at: $APP_DEST"
echo " To test it, run:"
echo "   open -a \"Typora Rust Reader\" sample.md"
echo "=================================================="
