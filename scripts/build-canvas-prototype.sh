#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail
cd "$(dirname "$0")/.."
cargo run -p otrios-dashboard --bin canvas-pack --features canvas --locked --offline
cargo build -p otrios-web-host --lib --target wasm32-unknown-unknown --release --locked --offline
canvas_target="${CARGO_TARGET_DIR:-target}"
wasm-bindgen --target web --out-name otrios_dashboard --out-dir web/canvas/pkg "$canvas_target/wasm32-unknown-unknown/release/otrios_web_host.wasm"
cargo build -p otrios-dashboard --features canvas,server,host-sync --bin canvas-server --bin canvas-native --locked --offline
