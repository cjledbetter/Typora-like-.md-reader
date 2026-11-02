# AGENT.MD — Multi-Agent Blueprint for `Typora-RustReader`

> **Project Name:** `Typora-RustReader` (`typora-rust-reader`)  
> **Target Platforms:** Omarchy Arch Linux (Wayland / Hyprland) & macOS  
> **Host Toolchain Detected:** `rustc 1.99.1`, `cargo 1.99.1`, `webkit2gtk-4.1`, `gtk4-wayland`, `libportal-gtk4` (All native C/GUI headers are already installed — **zero `sudo` installs required**).

---

## 1. Executive Summary & Architecture Decision

### The Problem with the Legacy `.py` + Chromium Build
1. **Chromium IPC Process Exit Bug:** Launching `chromium --app=http://127.0.0.1:8333/...` when Chromium is already open delegates the window to the existing browser process and exits in `<0.1s`, causing Python's `proc.wait()` to immediately shut down the HTTP server (`127.0.0.1 refused to connect`).
2. **Localhost TCP Overhead:** Spinning up an HTTP server on port `8333+` and polling `/api/watch` over HTTP every second is fragile and non-native.
3. **Relative Path & Desktop Entry Friction:** Opening files from different working directories or file managers can fail if the server lifecycle is tied to browser tab handoffs.

### The Native Rust Solution (`Typora-RustReader`)
We will build a **true standalone native Rust application** (`typora-rust-reader`) that eliminates Python, eliminates localhost HTTP ports, and eliminates Chromium `--app` hacks.

#### Recommended Architecture: Native Rust (`tao` + `wry`) + Embedded Asset Engine
Instead of re-implementing KaTeX math fonts, Mermaid SVG flowcharts, and print-to-PDF layout engines from scratch in raw OpenGL widgets, `Typora-RustReader` uses:
- **Windowing & Wayland/Hyprland Native Shell:** `tao` + `wry` (backed natively by `webkit2gtk-4.1` on Arch/Hyprland and `WKWebView` on macOS — the exact lightweight native OS windowing layer used by Tauri, with **zero Chromium dependency**).
- **Direct Rust File I/O & Native Dialogs:** `std::fs` + `rfd` (`libportal-gtk4` native Wayland file picker for Open/Save/Save As).
- **Kernel-Level Live File Watcher:** `notify` crate (`inotify` on Linux / `FSEvents` on macOS) pushing instant file-change events directly over the native Rust-to-UI IPC bridge (no HTTP polling).
- **Fast Rust Markdown & Metadata Engine:** `pulldown-cmark` for ultra-fast native AST parsing, word/character/reading-time calculation, heading extraction (Outline TOC), and standalone HTML export.
- **Single-Binary Embedded Assets:** `rust-embed` compiles the UI (`index.html`, KaTeX WOFF2 fonts, `highlight.js`, `mermaid.min.js`, and all 4 Typora themes) directly into the Rust executable so `typora-rust-reader` is a **single portable binary** with zero external runtime files.

---

## 2. Token Budget & Parallel Agent Strategy (Google Ultra Plan)

This project is **well within a single Google Ultra session allowance** (~40k–75k total tokens across all agents) because we do **not** need to rewrite the 84 KB Typora CSS/KaTeX/Mermaid rendering assets from scratch—we embed and upgrade them over a ~600-line native Rust core.

To build `Typora-RustReader` rapidly and cleanly, **4 specialized agents** can run in tandem (or sequentially in 4 focused phases):

```
┌──────────────────────────────────────────────────────────────────────────┐
│                     ORCHESTRATOR / LEAD ARCHITECT                        │
│     Coordinates workspace Cargo.toml, IPC contract, and final build      │
└───────┬──────────────────────┬──────────────────────┬────────────────────┘
        │                      │                      │
        ▼                      ▼                      ▼
┌───────────────────┐  ┌───────────────────┐  ┌────────────────────────────┐
│     AGENT 1       │  │     AGENT 2       │  │          AGENT 3           │
│  Rust Core & CLI  │  │ Native Window,    │  │ Frontend IPC & Live-Edit   │
│  File I/O, Notify │  │ Wry IPC & Dialogs │  │ Bridge (Edit/Split/Read)   │
└─────────┬─────────┘  └─────────┬─────────┘  └──────────────┬─────────────┘
          │                      │                           │
          └──────────────────────┼───────────────────────────┘
                                 ▼
                   ┌───────────────────────────┐
                   │          AGENT 4          │
                   │  Omarchy/Hyprland Desktop │
                   │  Installer & QA Suite     │
                   └───────────────────────────┘
```

---

## 3. Target Repository Layout

```text
/home/cjledbetter/Documents/Programming/typora-like-.md-reader/
├── AGENT.MD                        # This multi-agent architecture & execution plan
├── Cargo.toml                      # Rust package manifest for `typora-rust-reader`
├── src/
│   ├── main.rs                     # CLI args (clap), path resolution, app bootstrap
│   ├── state.rs                    # Thread-safe AppState (active file path, dirty flag, mtime)
│   ├── markdown.rs                 # pulldown-cmark TOC/heading extractor & stats engine
│   ├── watcher.rs                  # notify crate (inotify) kernel file watcher -> IPC event
│   ├── dialogs.rs                  # rfd native GTK4/Wayland Open & Save file dialogs
│   ├── assets.rs                   # rust-embed custom protocol (`typora://localhost/...`)
│   └── ipc.rs                      # JSON RPC bridge between Rust backend and Wry WebView
├── ui/                             # Embedded UI assets (bundled into binary at compile time)
│   ├── index.html                  # Typora-RustReader UI adapted for native `window.ipc`
│   └── assets/                     # Offline KaTeX, Mermaid, Highlight.js, WOFF2 fonts
├── packaging/
│   ├── typora-rust-reader.desktop  # Native Hyprland/Wayland XDG desktop entry
│   └── install-rust-omarchy.sh     # Installs binary to ~/.local/bin/typora-rust-reader & symlinks typora-reader
└── legacy-python/                  # Preserved original Python/HTML standalone files
```

---

## 4. Detailed Agent Workstreams & Contracts

### Agent 1: Rust Core, CLI Resolution, Markdown AST & Kernel Watcher
**Files Owned:** `Cargo.toml`, `src/main.rs` (CLI), `src/state.rs`, `src/markdown.rs`, `src/watcher.rs`

#### Responsibilities:
1. **`Cargo.toml` Dependencies:**
   - `tao = "0.30"` (Native cross-platform window creation & event loop)
   - `wry = "0.47"` (Native OS WebKitGTK / WKWebView rendering engine)
   - `clap = { version = "4.5", features = ["derive"] }` (CLI argument parser)
   - `notify = "6.1"` (Cross-platform kernel file-system notifications)
   - `pulldown-cmark = "0.12"` (Fast native Markdown AST & TOC parser)
   - `rfd = "0.15"` (Native OS file dialogs via `xdg-desktop-portal` / GTK4)
   - `rust-embed = "8.5"` + `mime_guess = "2.0"` (Compile-time static asset embedding)
   - `serde = { version = "1.0", features = ["derive"] }` + `serde_json = "1.0"`
2. **Smart CLI Path Resolution (`src/main.rs`):**
   - Accept `typora-rust-reader [FILE]` (plus `--mode <read|split|edit>` and `--theme <github|night|newsprint|gothic>`).
   - Correctly resolve relative paths (`./systemSummary.md`, `Documents/systemSummary.md`), tilde paths (`~/...`), and absolute paths regardless of which directory the user launched from.
   - If no file is passed, auto-discover `README.md` or `.md` files in `std::env::current_dir()`, or open an untitled scratchpad buffer ready to edit and save.
3. **Kernel File Watcher (`src/watcher.rs`):**
   - Watch the active file's parent directory using `notify::RecommendedWatcher`.
   - Debounce rapid save events (50ms) and ignore self-triggered saves (`Ctrl+S` inside the app).
   - Emit `UserEvent::FileChanged(PathBuf)` directly into the `tao` event loop so external edits from Neovim/VS Code/Agents hot-reload instantaneously without losing scroll position.

---

### Agent 2: Native Window (`tao`/`wry`), Custom Protocol & Native File Dialogs
**Files Owned:** `src/assets.rs`, `src/dialogs.rs`, `src/ipc.rs`, and `tao` EventLoop in `src/main.rs`

#### Responsibilities:
1. **Zero-Server Custom Protocol (`typora://`):**
   - Register `wry::WebViewBuilder::with_custom_protocol("typora")` backed by `rust-embed`.
   - Serve `typora://localhost/index.html` and `typora://localhost/assets/...` directly from memory—**0 TCP ports opened, 0 firewall prompts, 0 conflicts with running browsers**.
2. **Native IPC Command Handler (`window.ipc.postMessage`):**
   Define a clean JSON message contract between the UI and Rust:
   - `{"cmd": "ready"}` $\rightarrow$ Rust sends `window.__TYPORA_NATIVE__.onFileLoaded({ path, filename, content, mtime, stats, toc })`.
   - `{"cmd": "save_file", "content": "..."}` $\rightarrow$ Rust writes atomically to `active_file` on disk (or opens native `rfd::FileDialog::new().save_file()` if untitled) and replies with `onFileSaved({ path, filename, mtime })`.
   - `{"cmd": "save_file_as", "content": "..."}` $\rightarrow$ Opens native GTK4/Wayland Save As dialog, updates `active_file` watcher target, and saves.
   - `{"cmd": "open_file_dialog"}` $\rightarrow$ Opens native GTK4/Wayland Open dialog (`*.md`, `*.markdown`, `*.txt`), switches the `notify` watcher to the new path, and loads the document.
   - `{"cmd": "open_dropped_path", "path": "..."}` $\rightarrow$ Handles native file drag-and-drop onto the window.
   - `{"cmd": "set_title", "title": "..."}` $\rightarrow$ Updates the native Hyprland/OS window title bar (`systemSummary.md — Typora-RustReader` or `*systemSummary.md` when unsaved).
   - `{"cmd": "print_pdf"}` $\rightarrow$ Invokes native print / PDF export.

---

### Agent 3: Frontend Native IPC Bridge & Enhanced Read/Edit Experience
**Files Owned:** `ui/index.html` (cloned from existing `index.html` so original remains untouched)

#### Responsibilities:
1. **Dual-Transport Adapter (`Native Wry IPC` + `Fallback Standalone`):**
   - Replace the old `fetch('/api/file')`, `fetch('/api/save')`, and 1-second `setInterval('/api/watch')` polling loop with a clean `NativeBridge` object:
     - When running inside `Typora-RustReader` (`window.ipc` present), use instant event-driven native IPC (`window.ipc.postMessage(...)`) with zero polling.
     - Retain `FileReader` fallback so `ui/index.html` still works if opened standalone.
2. **Read / Split / Edit Mode Enhancements:**
   - Support seamless switching between:
     - **Read Mode** (Distraction-free Typora reader view with interactive checkboxes, copyable code blocks, KaTeX math, Mermaid diagrams, and ScrollSpy Outline).
     - **Split Mode** (Draggable 50/50 editor + live preview with synchronized scrolling).
     - **Pure Edit Mode** (Full-width Markdown editor with line numbers, active line highlight, `Ctrl+B` bold, `Ctrl+I` italic, `Ctrl+K` link, `Tab`/`Shift+Tab` indentation, and live word/character/line status bar).
   - **Unsaved Changes Indicator:** Show a subtle dot on the filename badge and window title (`• systemSummary.md`) when modified in Edit/Split mode, cleared immediately on `Ctrl+S`.

---

### Agent 4: Omarchy / Hyprland Packaging, Desktop Integration & Verification
**Files Owned:** `packaging/typora-rust-reader.desktop`, `packaging/install-rust-omarchy.sh`, `README-RUST.md`

#### Responsibilities:
1. **Release Build & Binary Installation:**
   - Compile with `cargo build --release` (with `strip = true` and `lto = "thin"` in `Cargo.toml` for a fast, compact binary).
   - Install binary to `~/.local/bin/typora-rust-reader` and optionally symlink `~/.local/bin/typora-reader` (preserving `typora-reader-legacy` for the Python version).
2. **Hyprland / XDG Desktop Entry (`typora-rust-reader.desktop`):**
   - Configure `Exec=/home/cjledbetter/.local/bin/typora-rust-reader %f` and `MimeType=text/markdown;text/x-markdown;text/plain;`.
   - Set `StartupWMClass=typora-rust-reader` so Hyprland window rules can style or float/tile it cleanly.
3. **End-to-End Verification Checklist:**
   - Verify opening `/home/cjledbetter/Documents/Business/Exit-Plan-Tax/Documents/systemSummary.md` works even while Chromium is open.
   - Verify opening relative paths from any directory (`typora-rust-reader Documents/systemSummary.md`).
   - Verify editing & saving (`Ctrl+S`) writes directly to disk and updates file mtime.
   - Verify external file modification (e.g., `echo >> file.md`) triggers instant `inotify` reload in `<50ms`.

---

## 5. How to Trigger This Build with Agents

When you are ready to build `Typora-RustReader`, open this folder in your session (or use `/teamwork`) and run:

> *"Execute the multi-agent build plan in `AGENT.MD` to create and install `Typora-RustReader`."*
