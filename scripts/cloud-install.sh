#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail
cd "$(dirname "$0")/.."
command -v rustup >/dev/null || {
  echo 'Install official rustup before running this script.' >&2
  exit 1
}
rustup toolchain install 1.90.0 --profile minimal --component rustfmt --component clippy
cargo fetch --locked
bash scripts/check.sh
