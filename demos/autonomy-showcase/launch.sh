#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKSPACE_DIR="$ROOT_DIR/demos/autonomy-showcase/workspace"
MARVIS_EXTENSION_DIR="$ROOT_DIR/apps/vscode-extension"
DEMO_EXTENSION_DIR="$ROOT_DIR/demos/autonomy-showcase/driver-extension"

if [[ "${1:-}" != "--no-build" ]]; then
  echo "[marvis-demo] Building target/debug/lite-code"
  cargo build --manifest-path "$ROOT_DIR/Cargo.toml"
fi

CODE_BIN="${CODE_CMD:-}"
if [[ -z "$CODE_BIN" ]]; then
  if [[ -x "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code" ]]; then
    CODE_BIN="/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code"
  elif command -v code >/dev/null 2>&1; then
    CODE_BIN="$(command -v code)"
  else
    echo "[marvis-demo] Could not find the VSCode CLI. Set CODE_CMD=/absolute/path/to/code." >&2
    exit 1
  fi
fi

if [[ ! -x "$ROOT_DIR/target/debug/lite-code" ]]; then
  echo "[marvis-demo] target/debug/lite-code was not built." >&2
  exit 1
fi

echo "[marvis-demo] Opening VSCode with Marvis and demo driver development extensions"
exec "$CODE_BIN" \
  --new-window \
  --extensionDevelopmentPath "$MARVIS_EXTENSION_DIR" \
  --extensionDevelopmentPath "$DEMO_EXTENSION_DIR" \
  "$WORKSPACE_DIR"

