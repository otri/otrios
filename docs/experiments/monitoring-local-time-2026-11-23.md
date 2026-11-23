<!-- SPDX-License-Identifier: Apache-2.0 -->
# Monitoring local collection time — 2026-11-23

Monitoring's **AS OF** label is a host-local projection of the canonical collection
timestamp. Browser workers use `Intl.DateTimeFormat` with the browser's detected
timezone and the collection date's daylight-saving offset. Japan displays `JST`.
Frame's native service uses `TZ=Asia/Tokyo` and Linux's timezone database. The
explicitly labelled chart axes remain UTC. Local formatting changes neither the
stored snapshot nor its Merkle root and requires no state protocol change.

Signed release: `92d1e353511984348ecaf2d04a69dfb9aa94cab5e8b59a75ec43fa871bbc129b`.
Native executable SHA-256:
`e2f464d882a156181f1ba4d7aef20e7138789d67f693e0044fe59bc3b89fc8f2`.
The complete signature/digest closure was verified before activation; all app
bundles and data contracts are byte-identical to the previous release. Compatible
code rollback `a0c4f20e9c0ae9fc641b83ffb8abbf02de7922e03182fd0040ce7ba8560ddde9`
remains available during development.

Validation performed:

- Existing local-owner regression passed with additional assertions that local
  labels leave the snapshot root unchanged, repeat labels do not invalidate the
  view, and stale timestamps cannot relabel a newer collection.
  Command: `cargo test --locked -p otrios-core --features canvas locally_owned_monitoring_opens_updates_and_restarts_without_server --lib`.
- Actual worker formatter checked under Node for Tokyo (including date rollover),
  UTC, New York standard/daylight time, India half-hour offset and invalid dates.
- `node --test --test-name-pattern 'context loss pauses' scripts/test-canvas-worker.mjs`:
  one passed. Updated mock defaults for the new accessor and existing focus/model
  accessors that had fallen behind the real host API. Worker syntax check passed.
- Release Wasm build and wasm-bindgen generation passed. ARM direct display,
  collector and bootstrap release build passed; only existing unused-code warnings.
- Deployed through the existing signed activation hook. Restarted Frame with its
  timezone drop-in, verified the running native executable against the signed
  digest, confirmed local Monitoring and live synchronization. Its first-frame
  capture visibly read **AS OF Nov 23 09:24 JST**, with UTC labels on the plots.

Technical records: `monitoring-local-time.json` and
`monitoring-local-time-verification.json` under the existing Mac
`~/Library/Application Support/OTriOSOrbStack/checks/` directory. Browser timezone
logic and Wasm compilation were checked; no interactive browser journey or broad
qualification suite was run for this presentation-only change.
