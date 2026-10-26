#!/usr/bin/env bash
# build.sh — release menagerie; install --house parks bash steward
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"
cargo build --release
BIN="$ROOT/target/release/menagerie"
echo "built: $BIN"
ls -la "$BIN"

if [[ "${1:-}" != "install" ]]; then
  exit 0
fi
shift || true
HOUSE=0
if [[ "${1:-}" == "--house" ]]; then
  HOUSE=1
fi

LIB="$HOME/.local/lib/faeos"
WRAP_SRC="$ROOT/scripts/menagerie"
mkdir -p "$LIB" "$HOME/bin"
cp -f "$BIN" "$LIB/menagerie"
chmod +x "$LIB/menagerie"
echo "installed engine → $LIB/menagerie"

FACE="$HOME/bin/menagerie"
if [[ -f "$FACE" ]] && head -n 1 "$FACE" | grep -qE 'python|bash'; then
  cp -f "$FACE" "$LIB/menagerie-face"
  chmod +x "$LIB/menagerie-face"
  echo "parked old steward → $LIB/menagerie-face"
fi
for extra in menagerie-registry.py menagerie-run menagerie-tui.py; do
  if [[ -f "$HOME/bin/$extra" ]]; then
    cp -f "$HOME/bin/$extra" "$LIB/$extra"
  fi
done

install_launcher() {
  local dest="$1"
  mkdir -p "$(dirname "$dest")"
  cp -f "$WRAP_SRC" "$dest"
  chmod +x "$dest"
}

install_launcher "$HOME/bin/menagerie"
if [[ "$HOUSE" -eq 1 ]]; then
  for kit in "$HOME/faeOS/bin" "$HOME/faeos/bin"; do
    if [[ -d "$kit" ]]; then
      install_launcher "$kit/menagerie"
    fi
  done
fi
echo "installed launcher → $HOME/bin/menagerie"
echo "rollback: $LIB/menagerie-face"
