#! usr/bin/bash

cd /Users/errdivine/ErrDivine/Rust/lite-code
cargo build --release

cd apps/vscode-extension
npx @vscode/vsce package
code --install-extension marvis-vscode-0.1.0.vsix --force