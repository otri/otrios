<!-- SPDX-License-Identifier: Apache-2.0 -->
# Frame-owned Monitoring replication — 2026-11-22

Implemented [RFC 0038](../rfcs/0038-local-monitoring-replication.md) and cut over
Monitoring collection from OrbStack to Frame. Frame commits typed Monitoring app
data locally and renders from that snapshot. A separate producer initiates an
outbound authorized `/session` connection; OrbStack durably stores the same root
and distributes it to authorized viewers. Chart geometry is derived on each host.

## Signed deployment

- Active release: `a0c4f20e9c0ae9fc641b83ffb8abbf02de7922e03182fd0040ce7ba8560ddde9`.
- Native executable SHA-256: `7076047a71fb2bcf9641549dc6fcb22594f4cc194cc4fefa32f4a6c971d4e60b`.
- Signed collector SHA-256: `2bf6c7f0621a44b1863f467626359c3e0a89382be3307ea0178b6017781dc228`.
- State codec: `otrios-canvas-sync-v42`; release-control opening envelope stays v1.
- Publisher signature and complete public/private closure verified on Mac and
  OrbStack before activation. Existing app contracts and every non-Monitoring
  app bundle are byte-identical to the baseline.
- Compatible pre-change code rollback retained at release
  `f129513ff9ad8280c1f458cf3c657a06c58dc6b0464dd3a553c7e8fc98b4323f`,
  with the corresponding authority binary under
  `/home/otri/otrios/code-rollback/monitoring-local-baseline/`.

Frame now runs `otrios-monitoring-local.timer` and
`otrios-monitoring-publish.service`. OrbStack's old collector timer is disabled.
The producer credential and Prometheus read credential were moved privately using
their existing identities; no new trust root or privilege was created. Frame's
viewing Member credential remains separate. Its local owner is explicitly enabled
by `OTRIOS_MONITORING_DIR`; ordinary viewers do not get durable private caches.
Linux, graphics, calibrated touch, existing authority/tunnel and signed-release
trust remain in place. No app data was restored from a backup during acceptance.

## Focused validation

Commands used the existing Rust 1.90 toolchain, cached ARM Docker builder and
OrbStack Linux source copy. The Mac helper only supplies the established build
environment and Cargo target directory.

```sh
python3 /tmp/shared-focus-command.py cargo test --locked -p otrios-core --features canvas monitoring --lib
python3 -m unittest scripts.test_dashboard_release.SignedReleases.test_monitoring_tool_is_in_signed_native_closure_and_cached_with_executable_mode
node --test scripts/test-dashboard-session.mjs
python3 /tmp/shared-focus-command.py bash scripts/build-dashboard-web.sh

docker exec -e CARGO_BUILD_JOBS=3 otrios-dashboard-spd-studio-1-builder \
  cargo build --release --locked -p otrios-dashboard --features direct,server,canvas \
  --bin dashboard-direct --bin dashboard-monitoring --bin session-bootstrap \
  --target armv7-unknown-linux-gnueabihf

orb -m otrios bash -lc 'cd /home/otri/otrios/source && CARGO_BUILD_JOBS=3 cargo build --locked -p otrios-dashboard --features canvas,server,host-sync --bin canvas-server --bin canvas-bundle --example bundle_app_probe'
```

Results: two focused core tests passed (canonical roots/gaps/replay rejection and
local owner open/update/restart), one signed-helper test passed including tamper
rejection, 15 session tests passed, release Wasm and ARM builds passed, and the
Linux authority/bundle/probe build passed. Existing unused-field/helper warnings
remain. After the live status correction below, the local-owner test and ARM
build passed again; the correction only changes native confirmation reporting.
No broad repository preflight or browser qualification campaign was run.

## Live acceptance

1. A real Frame collection produced a durable local snapshot; the native GLES2
   presentation reported `monitoring.local=true` with the exact snapshot root.
2. A separate authorized `bundle_app_probe` process on OrbStack mounted Monitoring
   and received the same typed root with `local=false`. An already-open read lease
   received revision 3 to 4 after a new Frame collection, without remounting.
3. An anonymous probe received neither private UI nor Monitoring data. Attempting
   publication with Frame's viewing token was rejected; the collector's legitimate
   ACK remained unchanged. The service grant is required independently of membership.
4. Stopped the actual `otrios-local.service` authority. While it remained stopped,
   triggered `otrios-monitoring-local.service` on Frame. Collection advanced from
   revision 6 to 7, with a newer collection time. The displayed local root was
   `00e02357e4739d4c839b1a7161fc11051e71a3549e74f1940dfb52ea23c51b32`;
   `state_confirmed=true`, `stream_ready=false`.
5. Restarted `otrios-dashboard-direct.service` while OrbStack was still stopped.
   A new native process presented that same saved root using GLES2, still confirmed
   and disconnected. The first-frame capture showed the complete real charts.
6. Restored OrbStack in the test's `finally` path. Frame's publisher reconnected
   outbound and received a durable ACK for that exact revision/root. A separate
   authorized reader then received the same revision 7 root through OrbStack.
7. `scripts/check-frame-release.py --subscriber-origin https://otrios.otri.ca`
   passed on the final signed release: running executable matched its signed
   digest, remote health was OK, zero pending commands, confirmed live state,
   GLES2/RGB565, 90-degree rotation, 800×1280 output and calibrated touch detected.
   Retired Frame authority/tunnel/legacy collector units stayed inactive.

The initial outage assertion exposed a native reporting bug: it required an online
server for every `state_confirmed` value, even a durable local owner. The corrected
host reports local confirmation independently while keeping remote leases
online-only. The initial bundle refresh also exposed `compose` rebuilding Home;
the new `refresh` operation retains authored Home UI. Signed closure checks proved
no unrelated app bundle changed.

Technical JSON evidence is retained in the existing Mac
`~/Library/Application Support/OTriOSOrbStack/checks/` directory:
`monitoring-local-outage.json`, `monitoring-peer-update.json`,
`monitoring-local-frame-release.json` and `monitoring-local-verification.json`.
No credentials or raw app snapshots are included in this record.

## Limits and rollback

This is one explicitly owned Monitoring producer and one authority replica, not
general multi-writer merging or automatic collector failover. Fresh collection
still needs Frame-to-Prometheus connectivity. If that fails, the saved **AS OF**
time remains unchanged; no automatic stale alarm is implemented. Other clients
need an authorized reachable server to receive updates. Local ownership is not
revocable cached browser access. Human touch and browser rendering were not
requalified in this task; shared Wasm compiled and actual separate-client data
replication was exercised.

During development, reverting to the v41 baseline requires its matching authority
binary and signed pointer together, stopping the new publisher/local timer and
removing the native local-owner drop-in before restarting the old display. The
former OrbStack collector can then resume against its legacy layout contract.
The legacy presentation file is retained for this compatible code rollback;
new typed app data does not need to be erased. Do not restore the retired Frame
authority, old tunnel or obsolete Frame state.
