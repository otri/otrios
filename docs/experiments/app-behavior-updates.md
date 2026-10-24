<!-- SPDX-License-Identifier: Apache-2.0 -->
# Independent app behavior validation — 2026-10-24

[RFC 0014](../rfcs/0014-app-behavior-updates.md) extends the shared app package
update path with compatible Wasm replacement. The initial runtime support is a
full release; subsequent guest edits compile only the selected app module.

## Automated checks

- `RUST_TEST_THREADS=1 bash scripts/check.sh`: formatting, strict workspace
  Clippy, 170 Rust tests and 39 Python checks pass.
- All 46 browser session/worker/cache/catalog/viewport/update/GPU tests pass,
  along with the Wasm release build, bindings and export check.
- ARM all-feature/all-target Clippy and release builds pass, including the direct
  host, authority, bootstrap and headless Canvas process probe.
- A stateful test guest proves document values survive a code update, replacement
  guest memory starts fresh, and repeating the catalog does not reset the guest.
- Rejected/trapping candidates retain the prior guest, scene and catalog. ABI,
  writable-field and simultaneous UI-contract changes are rejected.
- Partial-transfer tests reconstruct verified signed guest releases from the
  installed runtime and require the advertised behavior update capability.

## Acceptance experiment

The browser experiment keeps Playground and Home open in different orientations,
records the actual active guest hash, confirmed slider value and ownership, then
waits for a compatible guest release. The demonstration changes the linear slider
from continuous values to ten-percent steps. No UI source edit is needed.

```sh
node scripts/test-responsive-browser.mjs --url http://127.0.0.1:18877 \
  --wait-behavior-update --output target/app-behavior-browser
```

Results and live frame qualification are added after execution. Physical panel,
touch/color/performance acceptance remains separate.
