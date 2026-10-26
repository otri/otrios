#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
check_features=otrios-dashboard/server,otrios-dashboard/canvas,otrios-dashboard/host-sync,otrios-core/merkle
if [[ "${OTRIOS_CHECK_FIXED_UI_MATH:-0}" == 1 ]]; then
  check_features+=,otrios-core/fixed-ui-math
fi
cargo clippy --workspace --all-targets --features "$check_features" --locked --offline -- -D warnings
cargo test --workspace --features "$check_features" --locked --offline
export PYTHONPYCACHEPREFIX="${PYTHONPYCACHEPREFIX:-$PWD/target/python-cache}"
python3 scripts/test_dashboard_release.py
python3 scripts/test_dashboard_direct.py
python3 scripts/test_frame_hosting.py

python3 scripts/test-canvas-benchmark-counter.py

python3 scripts/test_app_publish.py
