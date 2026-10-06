#!/usr/bin/env python3
"""
Typora Reader - Universal Minimalist Markdown Reader
Compatible with macOS and Omarchy Arch Linux (Hyprland).
Zero external dependencies (uses standard Python 3 library only).
"""

import os
import sys
import json
import time
import socket
import argparse
import threading
import webbrowser
import subprocess
from pathlib import Path
from http.server import HTTPServer, SimpleHTTPRequestHandler
from urllib.parse import urlparse, parse_qs, unquote, quote

BASE_DIR = Path(__file__).resolve().parent
ACTIVE_FILE = None
ACTIVE_FILE_MTIME = None

def find_available_port(start_port=8333, max_attempts=50):
    """Find an open TCP port starting from start_port."""
    for port in range(start_port, start_port + max_attempts):
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            if s.connect_ex(('127.0.0.1', port)) != 0:
                return port
    return start_port

def resolve_target_file(file_arg, launch_cwd):
    """Resolve file path relative to launch directory or absolute.
    If no file_arg is provided, auto-detects README.md or .md files in launch_cwd."""
    if file_arg and file_arg.strip().lower() == "help":
        help_path = BASE_DIR / "help" / "README.md"
        if help_path.is_file():
            return help_path.resolve()

    if not file_arg:

        # 1. Check for standard readme files in current working directory
        for cand_name in ["README.md", "readme.md", "Readme.md", "index.md"]:
            cand = launch_cwd / cand_name
            if cand.is_file():
                return cand.resolve()

        # 2. Check if there are any .md files in the current working directory
        try:
            cwd_mds = sorted(list(launch_cwd.glob("*.md")))
            if len(cwd_mds) == 1:
                return cwd_mds[0].resolve()
            elif len(cwd_mds) > 1:
                # If multiple exist, return the first one
                return cwd_mds[0].resolve()
        except Exception:
            pass

        # 3. Fallback to sample.md in BASE_DIR
        sample_path = BASE_DIR / "sample.md"
        if sample_path.is_file():
            return sample_path.resolve()
        return None

    path_obj = Path(file_arg).expanduser()
    if not path_obj.is_absolute():
        path_obj = (launch_cwd / path_obj).resolve()
    else:
        path_obj = path_obj.resolve()

    return path_obj


class TyporaRequestHandler(SimpleHTTPRequestHandler):
    """Custom HTTP handler serving the reader UI and local file access API."""
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(BASE_DIR), **kwargs)

    def log_message(self, format, *args):
        # Suppress routine GET logging for cleaner terminal output
        if len(args) > 0 and "/api/watch" in str(args[0]):
            return
        super().log_message(format, *args)

    def end_headers(self):
        self.send_header("Cache-Control", "no-cache, no-store, must-revalidate")
        self.send_header("Pragma", "no-cache")
        self.send_header("Expires", "0")
        super().end_headers()

    def do_GET(self):
        parsed = urlparse(self.path)
        path = parsed.path
        query = parse_qs(parsed.query)

        # Root route: serve index.html
        if path == "/" or path == "/index.html":
            self.path = "/index.html"
            return super().do_GET()

        # API: Status
        if path == "/api/status":
            self.send_response(200)
            self.send_header("Content-Type", "application/json; charset=utf-8")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            status_data = {
                "active_file": str(ACTIVE_FILE) if ACTIVE_FILE else None,
                "filename": ACTIVE_FILE.name if ACTIVE_FILE else None,
                "mtime": ACTIVE_FILE.stat().st_mtime if ACTIVE_FILE and ACTIVE_FILE.is_file() else None
            }
            self.wfile.write(json.dumps(status_data).encode("utf-8"))
            return

        # API: Get File Content
        if path == "/api/file":
            target_path_str = query.get("path", [None])[0] or query.get("file", [None])[0]
            target_path = None
            if target_path_str:
                target_path = Path(unquote(target_path_str)).expanduser().resolve()
            elif ACTIVE_FILE:
                target_path = ACTIVE_FILE

            if not target_path or not target_path.is_file():
                self.send_response(404)
                self.send_header("Content-Type", "application/json; charset=utf-8")
                self.end_headers()
                self.wfile.write(json.dumps({"error": f"File not found: {target_path}"}).encode("utf-8"))
                return

            try:
                content = target_path.read_text(encoding="utf-8", errors="replace")
                stat = target_path.stat()
                response_data = {
                    "path": str(target_path),
                    "filename": target_path.name,
                    "content": content,
                    "mtime": stat.st_mtime,
                    "size": stat.st_size
                }
                self.send_response(200)
                self.send_header("Content-Type", "application/json; charset=utf-8")
                self.send_header("Cache-Control", "no-cache")
                self.end_headers()
                self.wfile.write(json.dumps(response_data).encode("utf-8"))
            except Exception as e:
                self.send_response(500)
                self.send_header("Content-Type", "application/json; charset=utf-8")
                self.end_headers()
                self.wfile.write(json.dumps({"error": str(e)}).encode("utf-8"))
            return

        # API: Watch for File Changes (Live-reload)
        if path == "/api/watch":
            target_path_str = query.get("path", [None])[0] or query.get("file", [None])[0]
            client_mtime = query.get("mtime", [None])[0]

            target_path = Path(unquote(target_path_str)).expanduser().resolve() if target_path_str else ACTIVE_FILE

            if not target_path or not target_path.is_file():
                self.send_response(404)
                self.end_headers()
                return

            try:
                current_mtime = target_path.stat().st_mtime
                modified = False
                if client_mtime is not None:
                    try:
                        modified = abs(float(client_mtime) - current_mtime) > 0.001
                    except ValueError:
                        modified = False

                self.send_response(200)
                self.send_header("Content-Type", "application/json; charset=utf-8")
                self.send_header("Cache-Control", "no-cache")
                self.end_headers()
                self.wfile.write(json.dumps({"modified": modified, "mtime": current_mtime}).encode("utf-8"))
            except Exception as e:
                self.send_response(500)
                self.end_headers()
            return

        # Serve static assets
        return super().do_GET()

    def do_POST(self):
        global ACTIVE_FILE, ACTIVE_FILE_MTIME
        parsed = urlparse(self.path)
        path = parsed.path

        # API: Save File
        if path == "/api/save":
            content_length = int(self.headers.get("Content-Length", 0))
            body = self.rfile.read(content_length).decode("utf-8")
            try:
                data = json.loads(body)
                target_path_str = data.get("path")
                content = data.get("content", "")

                target_path = None
                if target_path_str:
                    target_path = Path(unquote(target_path_str)).expanduser().resolve()
                elif ACTIVE_FILE:
                    target_path = ACTIVE_FILE

                if not target_path:
                    self.send_response(400)
                    self.send_header("Content-Type", "application/json; charset=utf-8")
                    self.end_headers()
                    self.wfile.write(json.dumps({"error": "No file path specified"}).encode("utf-8"))
                    return

                target_path.write_text(content, encoding="utf-8")
                new_mtime = target_path.stat().st_mtime

                if ACTIVE_FILE and target_path == ACTIVE_FILE:
                    ACTIVE_FILE_MTIME = new_mtime

                self.send_response(200)
                self.send_header("Content-Type", "application/json; charset=utf-8")
                self.send_header("Cache-Control", "no-cache")
                self.end_headers()
                self.wfile.write(json.dumps({"success": True, "mtime": new_mtime, "filename": target_path.name}).encode("utf-8"))
            except Exception as e:
                self.send_response(500)
                self.send_header("Content-Type", "application/json; charset=utf-8")
                self.end_headers()
                self.wfile.write(json.dumps({"error": str(e)}).encode("utf-8"))
            return

        self.send_response(404)
        self.end_headers()

def launch_browser(url, use_app_mode=True):
    """Launch browser, preferring app window mode on Omarchy/Linux/macOS."""
    # List of common browsers supporting --app flag for minimal Typora feel
    candidate_app_browsers = [
        "zen-browser",
        "google-chrome-stable",
        "google-chrome",
        "chromium",
        "brave-browser",
        "microsoft-edge"
    ]

    if use_app_mode and sys.platform.startswith("linux"):
        for b in candidate_app_browsers:
            cmd = [b, f"--app={url}"]
            try:
                subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                return True
            except FileNotFoundError:
                continue

    # Fallback to standard browser launch
    try:
        webbrowser.open(url)
        return True
    except Exception:
        return False

def main():
    global ACTIVE_FILE, ACTIVE_FILE_MTIME

    try:
        sys.stdout.reconfigure(line_buffering=True)
    except Exception:
        pass

    parser = argparse.ArgumentParser(
        description="Typora Reader - Minimalist Distraction-free Markdown Reader"
    )
    parser.add_argument("file", nargs="?", help="Path to markdown file to view")
    parser.add_argument("-p", "--port", type=int, default=8333, help="Port to listen on (default: 8333)")
    parser.add_argument("--no-browser", action="store_true", help="Do not open browser automatically")
    parser.add_argument("--standard-browser", action="store_true", help="Do not attempt --app mode")

    args = parser.parse_args()

    launch_cwd = Path.cwd()
    resolved = resolve_target_file(args.file, launch_cwd)
    if args.file and (not resolved or not resolved.is_file()):
        print(f"\033[31mError:\033[0m File not found: {args.file}", file=sys.stderr)
        sys.exit(1)

    if resolved and resolved.is_file():
        ACTIVE_FILE = resolved
        ACTIVE_FILE_MTIME = ACTIVE_FILE.stat().st_mtime


    port = find_available_port(args.port)
    server_address = ("127.0.0.1", port)
    
    try:
        httpd = HTTPServer(server_address, TyporaRequestHandler)
    except OSError as e:
        print(f"\033[31mFailed to start server on port {port}:\033[0m {e}", file=sys.stderr)
        sys.exit(1)

    params = []
    if ACTIVE_FILE:
        params.append(f"file={quote(str(ACTIVE_FILE))}")
    params.append(f"t={int(time.time())}")
    url = f"http://127.0.0.1:{port}/?" + "&".join(params)

    print("\033[1;36m===================================================\033[0m")
    print("\033[1;32m  Typora Reader  \033[0m(macOS & Omarchy Arch Linux)")
    print("\033[1;36m===================================================\033[0m")
    if ACTIVE_FILE:
        print(f" \033[1mViewing:\033[0m    {ACTIVE_FILE.name}")
        print(f" \033[1mPath:\033[0m       {ACTIVE_FILE}")
    print(f" \033[1mURL:\033[0m        \033[4m{url}\033[0m")
    print(f" \033[1mLive Reload:\033[0m \033[32mActive\033[0m (watching for file modifications)")
    print("\033[1;36m---------------------------------------------------\033[0m")
    print(" Press \033[1mCtrl+C\033[0m to stop the reader server.\n")

    if not args.no_browser:
        app_mode = not args.standard_browser
        threading.Timer(0.3, lambda: launch_browser(url, use_app_mode=app_mode)).start()

    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down Typora Reader.")
        httpd.server_close()
        sys.exit(0)

if __name__ == "__main__":
    main()
