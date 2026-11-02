# Porting Typora-RustReader to Apple Silicon (M1 / M2 / M3) macOS

This guide provides step-by-step instructions for transferring, compiling, and installing `Typora-RustReader` on your M1 MacBook Pro as a native Apple Silicon binary and native macOS `.app` bundle.

---

## 1. Why It Works Out of the Box on macOS

The codebase was architected using cross-platform Rust crates that automatically swap from Linux system libraries to Apple's native Cocoa frameworks at compile time:

| Linux / Hyprland Component | macOS / Cocoa Equivalent in Typora-RustReader |
| :--- | :--- |
| **Windowing & Event Loop** | `tao` (Wayland / X11) $\rightarrow$ `tao` (Native Cocoa `NSWindow`) |
| **Web Engine** | `wry` (WebKitGTK) $\rightarrow$ `wry` (Apple `WKWebView` with hardware acceleration) |
| **Live File Watcher** | `notify` (`inotify`) $\rightarrow$ `notify` (Apple `FSEvents` kernel monitoring) |
| **Native File Dialogs** | `rfd` (GTK4 / XDG Portal) $\rightarrow$ `rfd` (Native `NSOpenPanel` / `NSSavePanel`) |
| **Embedded Assets** | `rust-embed` (`typora://localhost/...`) works identically in-memory on both |

---

## 2. Step 1: Transfer the Project to Your Mac

Choose either **Git** or **Local Network Transfer (rsync / AirDrop)**:

### Option A: Via Git (Recommended)
1. On your Linux machine, commit and push your changes:
   ```bash
   cd ~/Documents/Programming/typora-like-.md-reader
   git add .
   git commit -m "feat: complete Typora-RustReader native build"
   git push origin main
   ```
2. On your M1 Mac terminal:
   ```bash
   mkdir -p ~/Documents/Programming
   cd ~/Documents/Programming
   git clone <YOUR_GIT_URL> typora-like-.md-reader
   cd typora-like-.md-reader
   ```

### Option B: Via rsync (Direct Local Network Transfer)
From your M1 Mac terminal (replace `arch-ip` with your Linux machine's local IP address):
```bash
mkdir -p ~/Documents/Programming
rsync -avz --exclude 'target' --exclude '.git' cjledbetter@<arch-ip>:~/Documents/Programming/typora-like-.md-reader/ ~/Documents/Programming/typora-like-.md-reader/
cd ~/Documents/Programming/typora-like-.md-reader
```

---

## 3. Step 2: Build the Apple Silicon Release Binary

Make sure you have Rust installed on your Mac (`rustup --version`). If not, install via:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then build the native ARM64 release binary:
```bash
cd ~/Documents/Programming/typora-like-.md-reader

# Compile optimized release binary (targets aarch64-apple-darwin automatically)
cargo build --release
```

This compiles a standalone, stripped executable at `target/release/typora-rust-reader` with all KaTeX fonts, Mermaid diagrams, Highlight.js, and themes embedded directly into the binary.

---

## 4. Step 3: Install to Terminal PATH (`~/.local/bin/`)

Install the binary and symlink so you can run `typora-reader` or `typora-rust-reader` from anywhere in your Mac terminal:

```bash
# 1. Create ~/.local/bin if not present
mkdir -p ~/.local/bin

# 2. Copy the binary
cp target/release/typora-rust-reader ~/.local/bin/typora-rust-reader
chmod +x ~/.local/bin/typora-rust-reader

# 3. Create the convenient symlink
ln -sf ~/.local/bin/typora-rust-reader ~/.local/bin/typora-reader
```

### Ensure `~/.local/bin` is in your macOS `$PATH`
Open `~/.zshrc` (or run):
```bash
if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
  echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
  source ~/.zshrc
fi
```

Now you can test it directly:
```bash
typora-reader sample.md
```

---

## 5. Step 4: (Optional) Create a Native `.app` macOS Application

To have a clickable macOS application in your `/Applications` folder that integrates with Finder, Spotlight, and the Dock:

1. **Install `cargo-bundle`**:
   ```bash
   cargo install cargo-bundle
   ```

2. **Add Bundle Metadata to `Cargo.toml`**:
   Add this section to the bottom of `Cargo.toml`:
   ```toml
   [package.metadata.bundle]
   name = "Typora Rust Reader"
   identifier = "com.typora.rust-reader"
   icon = ["assets/icon.svg"]
   version = "0.1.0"
   copyright = "Copyright (c) 2026 Typora-RustReader Team"
   category = "Productivity"
   short_description = "Minimalist distraction-free Markdown reader and editor"
   ```

3. **Build the Bundle**:
   ```bash
   cargo bundle --release
   ```

4. **Copy to `/Applications`**:
   ```bash
   cp -r target/release/bundle/osx/Typora\ Rust\ Reader.app /Applications/
   ```

### Set as Default Reader for `.md` Files in Finder:
1. In Finder, right-click any `.md` file $\rightarrow$ select **Get Info** (<kbd>Cmd</kbd> + <kbd>I</kbd>).
2. Under **Open with:**, select **Typora Rust Reader.app**.
3. Click **Change All...** to make it your system-wide default Markdown viewer.

---

## 6. CLI Usage Cheat Sheet on macOS

```bash
# Open file in default view:
typora-reader document.md

# Open directly in Split Screen (Editor + Live Preview):
typora-reader --mode split document.md

# Open directly in Full Editor view:
typora-reader --mode edit document.md

# Choose specific theme (github, night, newsprint, gothic):
typora-reader --theme night document.md
```
