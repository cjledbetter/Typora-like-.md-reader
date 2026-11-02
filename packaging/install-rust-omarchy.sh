#!/usr/bin/env bash
# ==============================================================================
# Installer for Typora Rust Reader on Omarchy Arch Linux (Hyprland) & Distros
# Feature 35: System Installation & Symlink Migration Script
# ==============================================================================
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ "$(basename "$SCRIPT_DIR")" = "packaging" ]; then
  PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
else
  PROJECT_ROOT="$SCRIPT_DIR"
fi

BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
APP_DIR="${APP_DIR:-$HOME/.local/share/applications}"
ICON_DIR="${ICON_DIR:-$HOME/.local/share/icons/hicolor/scalable/apps}"
TARGET_BIN="${TARGET_BIN:-$PROJECT_ROOT/target/release/typora-rust-reader}"

echo "=================================================="
echo " Installing Typora Rust Reader (Native Rust v2)"
echo "=================================================="

# 1. Ensure target directories exist
mkdir -p "$BIN_DIR" "$APP_DIR" "$ICON_DIR"

# 2. Verify or build release binary
if [ ! -f "$TARGET_BIN" ]; then
  echo "Release binary not found at $TARGET_BIN."
  if command -v cargo &>/dev/null; then
    echo "Building release binary via 'cargo build --release'..."
    (cd "$PROJECT_ROOT" && cargo build --release)
  else
    echo "Error: cargo not found and binary missing at $TARGET_BIN" >&2
    exit 1
  fi
fi

if [ ! -f "$TARGET_BIN" ]; then
  echo "Error: Failed to find or produce release binary at $TARGET_BIN" >&2
  exit 1
fi

# 3. Install binary to ~/.local/bin/typora-rust-reader (0755 permissions)
DEST_BINARY="$BIN_DIR/typora-rust-reader"
cp -f "$TARGET_BIN" "$DEST_BINARY"
chmod 0755 "$DEST_BINARY"
echo "✔ Installed binary to $DEST_BINARY (chmod 0755)"

# 4. Handle Symlink Migration (~/.local/bin/typora-reader -> typora-rust-reader)
SYMLINK="$BIN_DIR/typora-reader"
LEGACY_BACKUP="$BIN_DIR/typora-reader-legacy"

if [ -e "$SYMLINK" ] || [ -L "$SYMLINK" ]; then
  IS_TARGET_SYMLINK=false
  if [ -L "$SYMLINK" ]; then
    LINK_TARGET="$(readlink "$SYMLINK" 2>/dev/null || true)"
    if [ "$LINK_TARGET" = "typora-rust-reader" ] || [ "$LINK_TARGET" = "$DEST_BINARY" ]; then
      IS_TARGET_SYMLINK=true
    fi
  fi

  if [ "$IS_TARGET_SYMLINK" = false ]; then
    echo "Backing up existing legacy typora-reader to $LEGACY_BACKUP..."
    mv -f "$SYMLINK" "$LEGACY_BACKUP"
    echo "✔ Backed up legacy launcher to $LEGACY_BACKUP"
  else
    # Already points to typora-rust-reader; remove existing symlink to recreate cleanly
    rm -f "$SYMLINK"
  fi
fi

# Create symlink pointing directly to typora-rust-reader
(cd "$BIN_DIR" && ln -s "typora-rust-reader" "typora-reader")
echo "✔ Symlinked $SYMLINK -> typora-rust-reader"

# 5. Install desktop file
DESKTOP_SRC=""
if [ -f "$PROJECT_ROOT/packaging/typora-rust-reader.desktop" ]; then
  DESKTOP_SRC="$PROJECT_ROOT/packaging/typora-rust-reader.desktop"
elif [ -f "$PROJECT_ROOT/typora-reader.desktop" ]; then
  DESKTOP_SRC="$PROJECT_ROOT/typora-reader.desktop"
fi

if [ -n "$DESKTOP_SRC" ]; then
  cp -f "$DESKTOP_SRC" "$APP_DIR/typora-rust-reader.desktop"
  chmod +x "$APP_DIR/typora-rust-reader.desktop"
  echo "✔ Installed desktop entry to $APP_DIR/typora-rust-reader.desktop"
else
  echo "⚠ Warning: Desktop entry not found at packaging/typora-rust-reader.desktop"
fi

# 6. Install scalable icons
ICON_SRC=""
if [ -f "$PROJECT_ROOT/ui/assets/icon.svg" ]; then
  ICON_SRC="$PROJECT_ROOT/ui/assets/icon.svg"
elif [ -f "$PROJECT_ROOT/assets/icon.svg" ]; then
  ICON_SRC="$PROJECT_ROOT/assets/icon.svg"
fi

if [ -n "$ICON_SRC" ]; then
  cp -f "$ICON_SRC" "$ICON_DIR/typora-rust-reader.svg"
  cp -f "$ICON_SRC" "$ICON_DIR/typora-reader.svg"
  echo "✔ Installed scalable icons to $ICON_DIR"
else
  echo "⚠ Warning: Icon file not found in ui/assets/icon.svg or assets/icon.svg"
fi

# 7. Update desktop database if available
if command -v update-desktop-database &>/dev/null; then
  update-desktop-database "$APP_DIR" 2>/dev/null || true
  echo "✔ Updated desktop application database"
fi

# 8. Update GTK icon cache if available
if command -v gtk-update-icon-cache &>/dev/null; then
  gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
  echo "✔ Updated GTK icon cache"
fi

# 9. Associate text/markdown MIME types with Typora Rust Reader
if command -v xdg-mime &>/dev/null; then
  xdg-mime default typora-rust-reader.desktop text/markdown 2>/dev/null || true
  xdg-mime default typora-rust-reader.desktop text/x-markdown 2>/dev/null || true
  echo "✔ Associated markdown MIME types with typora-rust-reader.desktop (xdg-mime)"
fi

echo ""
echo "=================================================="
echo " Installation Complete!"
echo "=================================================="
echo "You can launch Typora Rust Reader via:"
echo " 1. Terminal:         typora-rust-reader [path/to/file.md]"
echo "                      or typora-reader [path/to/file.md]"
echo " 2. In Hyprland:      Press SUPER+SPACE, search 'Typora Rust Reader'"
echo " 3. File Manager:     Double-click any .md file (Yazi / Thunar / Dolphin)"
echo ""
