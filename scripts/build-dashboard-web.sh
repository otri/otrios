#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail
cd "$(dirname "$0")/.."
command -v wasm-bindgen >/dev/null || { echo 'Install wasm-bindgen-cli 0.2.129 (see docs/DASHBOARD.md)' >&2; exit 1; }
[[ "$(wasm-bindgen --version)" == 'wasm-bindgen 0.2.129' ]] || { echo 'wasm-bindgen-cli 0.2.129 required' >&2; exit 1; }
cargo build -p otrios-dashboard --lib --release --locked --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir web/dashboard/pkg target/wasm32-unknown-unknown/release/otrios_dashboard.wasm
