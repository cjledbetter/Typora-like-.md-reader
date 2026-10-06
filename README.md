# Typora Reader

A minimalist, distraction-free Markdown reader inspired by **Typora**. Designed to work seamlessly out-of-the-box on both **macOS** and **Omarchy Arch Linux** (Hyprland).

Zero external dependencies required (uses standard Python 3 and web technologies).

---

## ✨ Features

- **Split-Screen Live Markdown Editor (Live Markdown Preview)**:
  - Toggle between **Edit**, **Split**, and **Preview** views instantly.
  - Interactive split screen with raw Markdown editor on the left and live Typora-styled preview on the right.
  - **Draggable Resizer**: Click and drag the vertical divider to adjust editor vs. preview width (double-click to reset 50/50).
  - **Synchronized Scrolling**: Scroll either the editor or the preview and the other follows smoothly.
  - **Editor Enhancements**: Line number gutter, character/word counters, <kbd>Tab</kbd> / <kbd>Shift</kbd>+<kbd>Tab</kbd> indentations, auto-closing brackets, and copy/clear tools.
  - **Save to Disk**: Save edits directly to file via <kbd>Ctrl</kbd>/<kbd>Cmd</kbd> + <kbd>S</kbd> when using the Python server, or download/save in standalone mode.
- **Typora Look & Feel**: Beautiful typography, precise margins, and authentic Typora themes:
  - **GitHub** (Light, classic clean Typora default)
  - **Night** (Dark mode with soft slate accents)
  - **Newsprint** (Warm editorial paper with Georgia serif typography)
  - **Gothic** (High-contrast modern dark mode)
- **Table of Contents / Outline Sidebar**:
  - Automatically parses all heading levels (`#` through `######`).
  - Active **ScrollSpy**: smoothly highlights which section you are currently reading as you scroll.
  - Search/filter headings in real time.
- **Mathematics (KaTeX)**:
  - Full support for inline `$E = mc^2$` and display math `$$\int_{-\infty}^{\infty} e^{-x^2} dx = \sqrt{\pi}$$`.
  - **100% offline**: All KaTeX libraries and WOFF2 fonts are locally bundled in `assets/`.
- **Code Syntax Highlighting**:
  - Auto-language detection with `highlight.js`.
  - Language badge and one-click **Copy Code** button.
- **Interactive Task Lists**: Checkable GFM checkboxes (`- [x]`).
- **Diagrams**: Native Mermaid diagram rendering (flowcharts, sequence diagrams, state diagrams).
- **Live Auto-Reload**:
  - When you view a file via the CLI, the reader watches file modification times.
  - If you edit the file in **Neovim**, **Helix**, or **VS Code**, the reader immediately refreshes while preserving your scroll position!
- **Universal Drag & Drop**: Drag any `.md` file directly onto the window to view it.
- **In-Page Search (<kbd>Ctrl</kbd> + <kbd>F</kbd>)**:
  - Fast search with Next / Prev buttons, match counter (`3 of 12`), and yellow highlight markers.
- **Print / PDF Export (<kbd>Ctrl</kbd> + <kbd>P</kbd>)**:
  - Clean stylesheet formatted specifically for printing or exporting crisp PDFs without UI clutter.

---

## 🚀 Usage on macOS

### Option 1: Standalone Reader (Zero Setup)
Simply double-click `reader.html` to open it in Safari, Chrome, or your default browser.
- Drag and drop any `.md` file onto the window, or click **Open...** (<kbd>Cmd</kbd> + <kbd>O</kbd>).

### Option 2: Command Line with Live Reload
Open your terminal in this folder and run:
```bash
./typora-reader.sh sample.md
# Or directly with Python 3:
python3 typora-reader.py sample.md
```
This automatically launches your browser and connects live reload.

---

## 🐧 Usage on Omarchy Arch Linux (Hyprland)

Omarchy is David Heinemeier Hansson's (DHH) developer-focused Arch Linux + Hyprland environment. Typora Reader is tailored specifically for this setup:

### 1-Step Installation
Run the installer script:
```bash
./install-omarchy.sh
```

This will automatically:
1. Link `typora-reader` into your `~/.local/bin/` (accessible anywhere in your terminal).
2. Install the desktop entry `~/.local/share/applications/typora-reader.desktop` with an SVG icon.
3. Associate `.md` files so they can be opened from your file manager or Hyprland launcher.

### Launching on Omarchy:
- **From Terminal**:
  ```bash
  typora-reader ~/Notes/architecture.md
  ```
- **In Hyprland App Launcher** (<kbd>SUPER</kbd> + <kbd>SPACE</kbd> / Wofi / Rofi):
  Search for **Typora Reader**.
- **File Manager**:
  Click or open any `.md` file with Typora Reader.

> **Tip for Hyprland**: Typora Reader will automatically try to open in `--app` mode (Zen Browser / Chrome / Brave / Chromium) to give you a borderless, distraction-free native app window!

---

## ⌨️ Keyboard Shortcuts

| Shortcut | Action |
| :--- | :--- |
| <kbd>Ctrl</kbd> / <kbd>Cmd</kbd> + <kbd>\</kbd> (or <kbd>E</kbd>) | Toggle Split Screen / Preview Mode |
| <kbd>Ctrl</kbd> / <kbd>Cmd</kbd> + <kbd>S</kbd> | Save document changes to file / download |
| <kbd>Ctrl</kbd> / <kbd>Cmd</kbd> + <kbd>O</kbd> | Open local Markdown file |
| <kbd>Ctrl</kbd> / <kbd>Cmd</kbd> + <kbd>Shift</kbd> + <kbd>O</kbd> | Toggle Outline Sidebar (or <kbd>Ctrl</kbd>+<kbd>B</kbd>) |
| <kbd>Ctrl</kbd> / <kbd>Cmd</kbd> + <kbd>F</kbd> | Find text in document (Enter for next, Shift+Enter for prev) |
| <kbd>Ctrl</kbd> / <kbd>Cmd</kbd> + <kbd>P</kbd> | Print or Export to PDF |
| <kbd>Ctrl</kbd> / <kbd>Cmd</kbd> + <kbd>R</kbd> | Reload current document |
| <kbd>Esc</kbd> | Close search bar / dismiss overlay |

---

## 📁 Project Structure

```
typora-Like/
├── reader.html              # Standalone, offline-ready Typora Markdown reader
├── typora-reader.py         # Zero-dependency Python CLI server with live file watcher
├── typora-reader.sh         # Shell launcher script
├── typora-reader.desktop    # Linux desktop file for Omarchy Arch (Hyprland)
├── install-omarchy.sh       # 1-command installer for Arch / Omarchy
├── sample.md                # Comprehensive Typora showcase document
├── assets/                  # Fully bundled offline assets (KaTeX, highlight.js, marked, fonts)
│   ├── marked.min.js
│   ├── highlight.min.js
│   ├── highlight-github.min.css
│   ├── highlight-github-dark.min.css
│   ├── katex.min.js
│   ├── katex.min.css
│   ├── katex-auto-render.min.js
│   ├── mermaid.min.js
│   ├── icon.svg
│   └── fonts/               # KaTeX WOFF2 fonts
└── README.md                # Documentation
```
