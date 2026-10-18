#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail
cd "$(dirname "$0")/.."
cargo test -p otrios-core --features canvas --locked --offline
cargo clippy -p otrios-dashboard -p otrios-web-host --all-targets --features canvas,server,host-sync --locked --offline -- -D warnings
cargo test -p otrios-dashboard --features canvas,server,host-sync --test canvas_process --locked --offline
