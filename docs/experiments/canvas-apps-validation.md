<!-- SPDX-License-Identifier: Apache-2.0 -->
# Canvas app package prototype validation — 2026-10-19

This is the opt-in RFC 0004 prototype, separate from the deployed dashboard.
See [the app package guide](../../apps/README.md) for folder ownership and migration.

## Evidence

- `bash scripts/check.sh`: passes, including the existing authority process tests,
  sixteen release/cache Python checks and the direct-host check.
- `bash scripts/check-canvas-prototype.sh`: passes. Core: 72 unit tests and five
  Canvas integration tests; the latter reuse app-local UI/schema/WAT sources.
  The real process acceptance test starts a loopback server and three clients.
- Strict workspace Clippy with all targets/features passes on macOS.
- Browser Wasm build and both new JavaScript syntax checks pass.
- Browser viewer: Home opens Playground and Layout Lab; the visible UI/State refs
  change. Native-client level=730 appeared as 73% in the browser; a real pointer
  action changed it to 75%. Home navigation and package reload work. Screenshot:
  `outputs/otrios-canvas-apps.png` in the parent task output folder.
- Native macOS process: downloaded Playground UI/code, executed its guest,
  published level=730 and received authority acknowledgement; primitive mesh
  contained 6,714 vertices.
- ARMv7 release builds of `canvas-native` and `canvas-server` pass in the existing
  Linux builder, from an isolated source directory.
- Actual Cortex-A7 photo frame: a temporary **loopback** authority and headless
  client in a new `/run` folder loaded the same packages and guest. Result:
  `level=730 primitive_vertices=6714 acknowledged=true`. The existing
  `otrios-dashboard-direct.service` remained active. This is CPU/compositor and
  WebSocket execution evidence, **not** physical Canvas UI, GLES presentation,
  touch or FPS acceptance. No display ownership was taken.

The process test additionally proves:

1. Concurrent Playground subscribers see the same edit, while a Layout Lab
   subscriber receives zero messages for that edit.
2. Repacking changed WAT, then remounting the stable refs, executes the new
   behavior without rebuilding/restarting the host or authority.
3. Authority kill/restart restores acknowledged state into a fresh RAM client.
4. The native probe works against the real server process.
5. Wrong UI/State pairs and a foreign browser Origin are rejected before data.

Runtime tests cover independent State inputs reusing UI/code, stale input epochs,
selector agreement between slider geometry and percentage text, cache reuse with
fresh guest instances, conflict merging, snapshot retention, wrong scope/history,
protected CRDT operations hidden behind an unchanged final value, malformed
hash/ABI/imports/commands, memory limits and fuel exhaustion.

## Size and boundary notes

Generated Playground behavior is 210 bytes; its UI is 16,362 bytes. Layout Lab UI
is 10,928 bytes; Home UI is 9,720 bytes. The complete browser host Wasm is
4,664,929 bytes **uncompressed** and still includes the legacy adapter exports;
this is not a minimal standalone host size result. Sizes are for this build, not
performance claims. Package outputs are generated and ignored in Git.

The prototype raw-code cache retains at most eight 64 KiB blobs in RAM, plus one
active bounded guest. It does not change the deployed persistent artifact caches.
Documents have a 4,096-change and 512 KiB sync/load budget, with no history
compaction yet. Durable storage occurs only at the authority, before sync reply.

Only Home, Playground and Layout Lab are independently mounted here. Other app
manifests record legacy dependency locations; their existing live implementations
remain in the dashboard shell. Generic collection selectors, per-selector Merkle
subscriptions, automatic package update notifications, production signed app
admission, and rollback of a rejected remote mount remain later work. A failed
candidate retains the old view but suspends input until reconnect. The isolated
viewer accepts exact loopback Origin and locally configured packages; it must not
be exposed as the production app loader without the signed-admission milestone.
