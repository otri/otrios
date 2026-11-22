<!-- SPDX-License-Identifier: Apache-2.0 -->
# Frame becomes a remote subscriber — 2026-11-22

Frame is online and follows the existing OrbStack authority at
`https://otrios.otri.ca`. Its local authority, public tunnel and Monitoring
collector were stopped, disabled and removed, along with their obsolete code,
credentials, state and release stores. Existing Linux, graphics, SSH/Tailscale,
device identity, publisher trust and touch calibration remain. User Transfers,
guest identity and the touch calibration backup were preserved.

Signed release: `f129513ff9ad8280c1f458cf3c657a06c58dc6b0464dd3a553c7e8fc98b4323f`.
Native executable SHA-256:
`1e4d14546f44f2294d86e23878926a37f6e46943c21c40bab09761b504d920c9`.
All Web assets, 85 authority files, app contracts and the catalog match the
previous accepted release; the state protocol remains `otrios-canvas-sync-v41`.
Complete signature/digest verification passed before activation. The compatible
previous signed release remains available on the authority during development.

## Corrections discovered on hardware

- The old native client was rejected with HTTP 403 because it supplied no Origin
  on remote sessions. Bootstrap and the authenticated host now share explicit
  Origin construction without changing identity or authorization policy.
- The native service previously pulled in the old authority through `Wants`.
  Replace that dependency in the main unit with network/Tailscale; systemd
  dependency directives cannot be cleared using an empty drop-in assignment.
- Fresh downloads overwrote the overall cache budget with a per-resource size
  bound. Separate variables now preserve the intended 128 MiB cache budget.
- Obsolete local-app removal metadata was replaced with an initialized empty
  local namespace using the existing device utility.

## Validation performed

- `python3 /tmp/shared-focus-command.py cargo test --locked -p otrios-dashboard
  --features canvas,host-sync --bin session-bootstrap
  native_session_origin_matches_remote_and_loopback_endpoints`: 1 passed.
- `python3 -m unittest scripts.test_dashboard_release.SignedReleases.test_download_keeps_whole_cache_budget_separate_from_resource_limits`:
  1 passed, including a native download larger than the per-resource bound.
- `docker exec -e CARGO_BUILD_JOBS=3 otrios-dashboard-spd-studio-1-builder cargo
  build --release --locked -p otrios-dashboard --features direct,server,canvas
  --bin dashboard-direct --bin session-bootstrap
  --target armv7-unknown-linux-gnueabihf`: passed.
- Updated bootstrap negotiated the remote stable v1 hello on actual Frame.
- `scripts/check-frame-release.py --subscriber-origin https://otrios.otri.ca`
  passed before and after cleanup/restart: matching signed running executable,
  remote health OK, confirmed state, live stream ready, zero pending commands,
  native GLES2, rotation 90, RGB565, 800×1280 output and calibrated touch detected.
  Old hosting roles remained inactive and absent from the native service's Wants.
- Frame's first-frame capture showed the new server's Home launcher. Physical
  touch gestures and individual chat effects were not exercised in this cutover.

Cleanup reclaimed 2,347,151,360 bytes (about 2.35 GB); root filesystem usage fell
from 84% to 27%, leaving about 2.9 GiB available. It removed retired server stores,
old client binaries/backups and benchmark/qualification captures. Runtime checks
and cleanup/signature records are under the existing Mac OTriOSOrbStack checks
directory. No broad qualification suite was run for this focused host cutover.
