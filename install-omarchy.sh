#!/usr/bin/env bash
# ==============================================================================
# Installer for Typora Reader on Omarchy Arch Linux (Hyprland) & other distros
# ==============================================================================
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="$HOME/.local/bin"
APP_DIR="$HOME/.local/share/applications"
ICON_DIR="$HOME/.local/share/icons/hicolor/scalable/apps"

echo "=================================================="
echo " Installing Typora Reader for Omarchy / Arch Linux"
echo "=================================================="

# 1. Ensure target directories exist
mkdir -p "$BIN_DIR" "$APP_DIR" "$ICON_DIR"

# 2. Make launcher scripts executable
chmod +x "$SCRIPT_DIR/typora-reader.py"
chmod +x "$SCRIPT_DIR/typora-reader.sh"

# 3. Create symlink in ~/.local/bin
ln -sf "$SCRIPT_DIR/typora-reader.sh" "$BIN_DIR/typora-reader"
echo "✔ Installed binary to $BIN_DIR/typora-reader"

# 4. Install icon
cp -f "$SCRIPT_DIR/assets/icon.svg" "$ICON_DIR/typora-reader.svg"
echo "✔ Installed icon to $ICON_DIR/typora-reader.svg"

# 5. Generate and install desktop file with absolute paths
cat > "$APP_DIR/typora-reader.desktop" <<EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=Typora Reader
GenericName=Markdown Reader
Comment=Minimalist distraction-free Markdown reader inspired by Typora
Exec=python3 "$SCRIPT_DIR/typora-reader.py" %f
Icon=typora-reader
Terminal=false
Categories=Office;WordProcessor;Utility;TextEditor;Viewer;
MimeType=text/markdown;text/x-markdown;text/plain;
Keywords=markdown;typora;reader;viewer;notes;
StartupNotify=true
StartupWMClass=typora-reader
EOF
chmod +x "$APP_DIR/typora-reader.desktop"
echo "✔ Installed desktop entry to $APP_DIR/typora-reader.desktop"

# 6. Update desktop database if available
if command -v update-desktop-database &>/dev/null; then
  update-desktop-database "$APP_DIR" 2>/dev/null || true
  echo "✔ Updated desktop application database"
fi

# 7. Associate text/markdown with Typora Reader
if command -v xdg-mime &>/dev/null; then
  xdg-mime default typora-reader.desktop text/markdown
  xdg-mime default typora-reader.desktop text/x-markdown
  echo "✔ Associated .md files with Typora Reader (xdg-mime)"
fi

echo ""
echo "=================================================="
echo " Installation Complete!"
echo "=================================================="
echo "You can now open markdown files in multiple ways:"
echo ""
echo " 1. Terminal:         typora-reader path/to/document.md"
echo " 2. In Hyprland:      Press SUPER+SPACE (or your app launcher) and search 'Typora Reader'"
echo " 3. File Manager:     Double-click any .md file (Yazi / Thunar / Dolphin)"
echo " 4. Direct Browser:   Open $SCRIPT_DIR/reader.html in any browser"
echo ""
