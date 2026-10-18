#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail
cd "$(dirname "$0")/.."
cargo run -p otrios-dashboard --bin canvas-pack --features canvas --locked --offline
cargo build -p otrios-dashboard --lib --target wasm32-unknown-unknown --features canvas --release --locked --offline
wasm-bindgen --target web --out-dir web/canvas/pkg target/wasm32-unknown-unknown/release/otrios_dashboard.wasm
cargo build -p otrios-dashboard --features canvas,server,host-sync --bin canvas-server --bin canvas-native --locked --offline
