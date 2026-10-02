#!/usr/bin/env bash
# Typora Reader Launcher for macOS and Omarchy Arch Linux (Hyprland)

# Resolve actual directory of this script, handling symlinks
SOURCE="${BASH_SOURCE[0]}"
while [ -h "$SOURCE" ]; do
  DIR="$( cd -P "$( dirname "$SOURCE" )" >/dev/null 2>&1 && pwd )"
  SOURCE="$(readlink "$SOURCE")"
  [[ $SOURCE != /* ]] && SOURCE="$DIR/$SOURCE"
done
SCRIPT_DIR="$( cd -P "$( dirname "$SOURCE" )" >/dev/null 2>&1 && pwd )"

# Ensure python3 is available
if ! command -v python3 &>/dev/null; then
  echo "Error: python3 is required to run Typora Reader." >&2
  exit 1
fi

exec python3 "$SCRIPT_DIR/typora-reader.py" "$@"
