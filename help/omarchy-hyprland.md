# Omarchy Arch Linux & Hyprland Integration Guide

This guide covers installing and running **Typora Reader** on **Omarchy** (David Heinemeier Hansson's developer-focused Arch Linux + Hyprland distribution).

---

## 1. Prerequisites on Omarchy

- **Python 3**: Standard in Arch Linux (pre-installed).
- **Web Browser**: Any browser (Zen Browser, Chromium, Google Chrome, Brave, or Firefox).
- **Zero package managers required**: No `npm`, `yarn`, or `pip` packages needed.

---

## 2. Transferring Files to Omarchy

Choose whichever file transfer method fits your workflow:

### Option A: Via Git
```bash
# On your macOS machine:
cd ".../Programming/typora-Like"
git init
git add .
git commit -m "Typora reader setup"
# Push to your GitHub / GitLab repository

# On your Omarchy machine:
git clone <your-repo-url> ~/Programming/typora-Like
```

### Option B: Via Local Network (rsync)
```bash
# From macOS terminal (replace user and omarchy-ip):
rsync -avz ".../Programming/typora-Like/" user@omarchy-ip:~/Programming/typora-Like/
```

---

## 3. Running the 1-Step Installer

On Omarchy, open your terminal in the directory and run:

```bash
cd ~/Programming/typora-Like
./install-omarchy.sh
```

### What this script automatically configures:
1. **CLI Symlink**: Links `typora-reader` into `~/.local/bin/typora-reader` (accessible globally).
2. **Desktop File**: Generates and installs `~/.local/share/applications/typora-reader.desktop`.
3. **App Icon**: Installs the vector SVG icon into `~/.local/share/icons/hicolor/scalable/apps/typora-reader.svg`.
4. **MIME Association**: Registers `text/markdown` via `xdg-mime` so double-clicking `.md` files in **Yazi**, **Thunar**, or **Dolphin** opens them in Typora Reader.
5. **Desktop Database**: Updates the system cache with `update-desktop-database`.

---

## 4. Hyprland Integration & App Mode

### Standalone Native Window (App Mode)
On Omarchy, the CLI automatically detects browsers that support the `--app` flag (such as **Zen Browser**, **Google Chrome**, **Chromium**, or **Brave**).

When you run:
```bash
typora-reader notes.md
```
It launches as an **isolated, tabless, distraction-free application window** without the browser address bar or tabs. It tiles perfectly in Hyprland!

### Hyprland Window Rules (Optional)
If you want Typora Reader to float or open at a specific window size in your Hyprland configuration, add this to `~/.config/hypr/hyprland.conf`:

```ini
# Float Typora Reader when opened
windowrulev2 = float, class:^(typora-reader|chrome-.*-Default|zen-.*)$
windowrulev2 = size 1000 850, class:^(typora-reader)$
windowrulev2 = center, class:^(typora-reader)$
```

### Launching via App Launcher
Press <kbd>SUPER</kbd> + <kbd>SPACE</kbd> (or your configured launcher like **Wofi**, **Rofi**, or **Walker**), type `Typora`, and select **Typora Reader**.
