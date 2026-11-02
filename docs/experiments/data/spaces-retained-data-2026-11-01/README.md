<!-- SPDX-License-Identifier: Apache-2.0 -->
# Retained-data acceptance evidence

JSON contains technical status and aggregate checks only. Screenshots use the
disposable browser fixture. Frame pixels, private inventories, credentials,
Space/collection mappings and app payloads remain outside the repository.

The browser fixture was captured before the final commit, so its original
base/dirty fields are preserved. `production-match.json` verifies 179 production
files against the frozen signed candidate. Post-freeze changes corrected one
obsolete test assertion and completed acceptance tooling; runtime/Web bytes did
not change. The deployed Wasm digest matches the browser report.
