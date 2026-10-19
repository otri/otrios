#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail
cd "$(dirname "$0")/.."
command -v wasm-bindgen >/dev/null || { echo 'Install wasm-bindgen-cli 0.2.129 (see docs/DASHBOARD.md)' >&2; exit 1; }
[[ "$(wasm-bindgen --version)" == 'wasm-bindgen 0.2.129' ]] || { echo 'wasm-bindgen-cli 0.2.129 required' >&2; exit 1; }
cargo run -p otrios-dashboard --bin canvas-pack --features canvas --locked
cargo build -p otrios-web-host --lib --release --locked --target wasm32-unknown-unknown
wasm-bindgen --target web --out-name otrios_dashboard --out-dir web/canvas/pkg "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/otrios_web_host.wasm"
