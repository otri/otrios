#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
check_features=otrios-dashboard/server,otrios-dashboard/canvas,otrios-dashboard/host-sync,otrios-core/merkle
cargo clippy --workspace --all-targets --features "$check_features" --locked --offline -- -D warnings
cargo test --workspace --features "$check_features" --locked --offline
export PYTHONPYCACHEPREFIX="${PYTHONPYCACHEPREFIX:-$PWD/target/python-cache}"
python3 scripts/test_dashboard_release.py
python3 scripts/test_dashboard_direct.py
python3 scripts/test_frame_hosting.py
for script in scripts/test-canvas-worker.mjs scripts/test-app-transfers.mjs scripts/test-dashboard-session.mjs scripts/test-dashboard-monitoring-session.mjs; do
  node "$script"
done

python3 scripts/test-canvas-benchmark-counter.py
python3 scripts/test-canvas-pipeline-cli.py
python3 scripts/test-clean-artifacts.py
python3 scripts/test-maintain-dashboard-builder.py

python3 scripts/test_app_publish.py
python3 crates/otrios-core/tests/fixtures/verify-portable-data-fixtures.py

cargo build -p otrios-dashboard --features server,canvas,host-sync --bin canvas-bundle --bin canvas-pack --bin dashboard-server --example bundle_counter --example bundle_app_probe --example app_management_probe --example space_ink_probe --example space_chat_probe --locked --offline
OTRIOS_SPACES_SERVER="$(realpath "${CARGO_TARGET_DIR:-$PWD/target}/debug/dashboard-server")" cargo test -p otrios-core --features canvas --test spaces_process --locked --offline
OTRIOS_TEST_BIN_DIR="${CARGO_TARGET_DIR:-$PWD/target}/debug" python3 scripts/test_app_bundles.py

if [[ "$(uname -s)" == Linux ]]; then
  bash scripts/test-linux-touch.sh
fi
