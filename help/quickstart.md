# Quickstart Guide

Learn how to use `typora-reader` from any terminal or folder.

---

## 1. Running From Any Directory

The `typora-reader` binary is installed in `~/.local/bin/typora-reader`, which is in your system `$PATH`. You do not need to `cd` into the project directory to use it.

### Automatic Detection (Zero Arguments)
If you run `typora-reader` without specifying a file:
```bash
typora-reader
```
The reader checks your **current working directory** for:
1. `README.md` / `readme.md` / `index.md`
2. Any single `.md` file present in the folder
3. Falls back to the welcome showcase document if no markdown file is found

---

## 2. Opening Specific Files

### Current Directory
```bash
typora-reader notes.md
```

### Relative Paths
```bash
typora-reader ../sibling-project/documentation.md
typora-reader ./subfolder/spec.md
```

### Home Directory Expansion (`~`)
```bash
typora-reader ~/Notes/todo.md
```

### Absolute Paths (including paths with spaces)
```bash
typora-reader "/Users/chris/Library/Mobile Documents/com~apple~CloudDocs/file.md"
```

---

## 3. Command Line Options

```bash
# Specify a custom local port (default is 8333, or next free port):
typora-reader -p 9000 notes.md

# Start the live-reload server without automatically popping open a browser window:
typora-reader --no-browser notes.md

# Open in standard browser instead of attempting app-mode:
typora-reader --standard-browser notes.md
```

---

## 4. Live Auto-Reload Workflow

When you open a file using `typora-reader`:
1. The reader serves the file and monitors its modification time.
2. Edit your markdown file in your favorite text editor (**Neovim**, **Helix**, **VS Code**, or **Nano**).
3. The moment you save (`:w` or <kbd>Cmd</kbd>+<kbd>S</kbd>), Typora Reader automatically refreshes the rendered view in your browser.
4. **Scroll position is preserved**, so you don't lose your place while writing!

To stop the background server, press <kbd>Ctrl</kbd> + <kbd>C</kbd> in the terminal where it is running.
