#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail
cd "$(dirname "$0")/.."
fixture=$(mktemp /tmp/otrios-touch-fixture.XXXXXX)
trap 'rm -f "$fixture"' EXIT
"${CC:-cc}" -std=c11 -O2 -Wall -Wextra -Werror crates/otrios-linux-display/tests/touch.c -o "$fixture"
"$fixture"
