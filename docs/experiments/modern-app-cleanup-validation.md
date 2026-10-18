<!-- SPDX-License-Identifier: Apache-2.0 -->
# Modern app cleanup and dashboard touchups — 2026-10-19

See [the implementation/retirement map](../MODERN_APP_PATTERN.md) for ownership,
the proposed cutover and features that are still absent. This pass does not move
the production dashboard onto the experimental Canvas package loader.

## Validation

- Required `scripts/check.sh` passes: 84 core unit tests, 11 Canvas integration
  tests, authority process checks, 16 signing/cache checks and the direct-host
  source boundary check, plus the remaining workspace tests.
- `scripts/check-canvas-prototype.sh` passes, including real authority/client
  processes, app isolation, changed guest execution, persisted recovery and the
  native headless client. Strict workspace/all-target/all-feature Clippy passes.
- Both browser Wasm hosts build. JavaScript syntax checks and 17 export,
  monitoring-session, release-gate and artifact-cache tests pass.
- All three native state-worker tests pass, including real WebSocket edits and
  reconnect recovery with the presentation/authority boundary.
- The Canvas-only host's raw Wasm exports contain Canvas but no Dashboard or
  MotionPresenter adapters. Existing unselected-state/history acknowledgements
  retain the same scene allocation; a selected-value edit replaces it.
- Browser interaction against an isolated loopback authority loads the new
  `host.js`/`host_bg.wasm`, opens Component Playground and executes its loaded
  Wasm toggle. The resulting state is acknowledged without browser console errors.
- Browser interaction against the updated dashboard host shows six Home cards,
  no Home navigation bar/footer, Roboto glyphs, independent `#/layout` routing,
  revised labels and a visible marker for one timestamped toggle.
- Font generation is reproducible (`tools/fonts/generate_roboto.py --check`).
  Font tests verify finite geometry, proportional advances, whitespace/fallback,
  clipping and deterministic meshes. No runtime font dependency was added;
  `font8x8` was removed.

Home subtitle size and singular touch-count copy were checked afterward with the
binary asset fixture test and the twelve targeted dashboard tests. Native frame
GLES/touch/FPS acceptance must be distinguished from these host/process tests.

## Timed signal

The authority still stores the original CRDT history. Each new discrete toggle
records its interaction timestamp in its Automerge change metadata. Pending
native commands retain that timestamp rather than substituting worker delivery
time. The presentation layer queries a five-minute window from an immutable
sample index, rebuilt only when sample-targeting changes arrive. Layout Lab's
scene invalidates once per wall-clock second, even without incoming state edits.
Animation, input repeat and trail timers continue using monotonic time.

Tests cover elapsed-time placement and expiry without new events or document
writes; concurrent sync, reconnect and reload preserving original event times;
more than 32 visible events; unchanged index reuse after slider edits; bounded
dense history; a visible single-event stroke; immutable old scenes; and queued
commands that must not make an old event young again.

The display retains the latest event per two-second visual bucket, at most 151
points. Timestamps are advisory device wall time and do not decide merge order
or authorization. Unsynchronized clocks can shift event placement. Historical
records with timestamp zero are omitted rather than given fabricated dates.
The existing durable document/history limits remain; five-minute visual expiry
is not history compaction or a durable retention policy.

## Published acceptance

The established watcher built ARMv7 direct and SDL recovery executables and
published signed release
`6c06abee17965e6b2a8c75a34ce08da2983efa8501a1b494f79a59c84bf4ddd1`.
The public browser automatically moved to it. Home, Component Playground and the
independent Layout route were checked there; toggling produced a time-positioned
graph and the shared toggle was returned to its starting value. Browser console
errors were absent. The deployed module/access conformance check passes all 45
checks, including public code/demo access and private Monitoring session,
revocation and admin-route isolation.

The frame service reports this exact signed release ACTIVE, with one native
dashboard child using direct DRM/GBM/EGL/GLES2 and RGB565 scanout. This proves
native build/activation and transport connection; physical touch, readability
and a new FPS measurement were not manually accepted in this pass. The user-facing
Home and graph screenshots are in the parent task's outputs as
`otrios-modern-home.jpg` and `otrios-toggle-signal.jpg`.

## Size evidence

| Build snapshot | Uncompressed generated Wasm bytes |
| --- | ---: |
| Combined prototype host before this cleanup | 4,679,734 |
| Canvas-only host before the font change | 4,300,032 |
| Canvas-only host with Roboto and final runtime changes | 4,409,550 |
| Live dashboard host with Roboto and timed signal | 3,240,306 |

Removing legacy exports alone saved 379,702 bytes (8.1%) in that intermediate
comparison. The final host adds vector font data and other changes; these are
build snapshots, not a controlled compiler benchmark, heap measurement or FPS
claim. Generated packages are ignored in Git. Roboto's source, hashes, notices,
bounded glyph coverage and regeneration instructions are in
[assets/fonts/roboto](../../assets/fonts/roboto/README.md).

## Remaining boundaries

The modern prototype's selected-value invalidation is not Merkle subtree sync.
The deployed Layout Lab now has a dedicated UI asset and launcher route, while
its toggle still shares the legacy room's enabled field. Fully independent
production app roots, signed graph admission and staged reload/rollback remain
the next cutover, rather than another layer of permanent screen adapters.
