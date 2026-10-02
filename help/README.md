# Typora Reader — Help Center & Documentation

Welcome to the **Typora Reader** documentation library. This directory contains quick references, installation instructions, and advanced tips for both macOS and Omarchy Arch Linux.

---

## 📚 Table of Contents

| Guide | Description |
| :--- | :--- |
| [**1. Quickstart Guide**](quickstart.md) | Terminal commands, running from any folder, and path handling |
| [**2. Omarchy Arch Linux Guide**](omarchy-hyprland.md) | 1-step installer, Hyprland app mode, and desktop launcher setup |
| [**3. Keyboard Shortcuts**](shortcuts.md) | All hotkeys for navigation, outline toggle, search, and PDF export |
| [**4. Markdown Syntax & Typora Features**](syntax-guide.md) | KaTeX math formatting, code highlighting, tables, and Mermaid |

---

## ⚡ Quick Cheat Sheet

```bash
# 1. Run in current folder (auto-detects local .md or README.md)
typora-reader

# 2. Open a specific file
typora-reader document.md

# 3. Open with relative or absolute paths
typora-reader ../other-folder/notes.md
typora-reader ~/Documents/architecture.md
typora-reader "/Users/chris/path with spaces/file.md"

# 4. Direct offline browser mode (no terminal needed)
open reader.html  # macOS
xdg-open reader.html  # Linux / Omarchy
```

---

## 💡 Top Features

- **Iconic Typora Themes**: GitHub (Light), Night (Dark), Newsprint (Georgia Serif), Gothic (Modern Dark).
- **Auto-Reload**: Saves made in Neovim, Helix, or VS Code reflect in the reader immediately while preserving scroll position.
- **Offline Mathematics**: KaTeX math formulas render 100% offline without network connections.
- **Drag & Drop**: Drag any `.md` file anywhere onto the reader window to open it.
- **Clean PDF Export**: Press <kbd>Cmd</kbd> / <kbd>Ctrl</kbd> + <kbd>P</kbd> to export distraction-free PDFs.
