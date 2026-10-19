#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features otrios-dashboard/server,otrios-dashboard/canvas,otrios-dashboard/host-sync,otrios-core/merkle --locked --offline -- -D warnings
cargo test --workspace --features otrios-dashboard/server,otrios-dashboard/canvas,otrios-dashboard/host-sync,otrios-core/merkle --locked --offline
export PYTHONPYCACHEPREFIX="${PYTHONPYCACHEPREFIX:-$PWD/target/python-cache}"
python3 scripts/test_dashboard_release.py
python3 scripts/test_dashboard_direct.py

python3 scripts/test-canvas-benchmark-counter.py
