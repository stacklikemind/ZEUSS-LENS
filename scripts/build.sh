#!/bin/sh
set -eu
PROJECT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$PROJECT_DIR"
cargo build --locked --release --target wasm32-unknown-unknown --lib
cp target/wasm32-unknown-unknown/release/vision_code_wasm.wasm web/vision_code.wasm
printf '%s\n' 'Built web/vision_code.wasm' 'Run: node scripts/serve.mjs' 'Open: http://localhost:8766/'
