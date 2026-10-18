#!/bin/sh
set -eu
PROJECT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$PROJECT_DIR"
cargo rustc --manifest-path desktop/Cargo.toml --locked --target wasm32-unknown-unknown --release --lib -- -C link-arg=-zstack-size=8388608
mkdir -p web/gpui/pkg
cp desktop/web/index.html desktop/web/loader.js desktop/web/shell.css web/gpui/
wasm-bindgen desktop/target/wasm32-unknown-unknown/release/optic_forge_ui.wasm --target web --out-dir web/gpui/pkg --no-typescript

cp desktop/web/index.html desktop/web/loader.js desktop/web/shell.css web/
mkdir -p web/pkg
cp web/gpui/pkg/* web/pkg/

cat desktop/THIRD_PARTY.md desktop/fonts/OFL.txt desktop/licenses/Apache-2.0.txt desktop/licenses/Lucide.txt > web/gpui/THIRD-PARTY-NOTICES.txt
cp web/gpui/THIRD-PARTY-NOTICES.txt web/
