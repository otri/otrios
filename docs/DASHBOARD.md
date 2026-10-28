<!-- SPDX-License-Identifier: Apache-2.0 -->
# Dashboard apps

OTriOS runs the same data-driven apps in a browser and on the Rockchip photo
frame. The frame owns DRM/GBM/EGL/OpenGL ES 2 directly; it runs no browser,
Wayland compositor or SDL host. The browser uses WebGL. Both consume the shared
Rust primitive compositor and cached Slug font renderer.

## App boundary

Home mounts `Canvas(uiRef, stateRef)` from an admitted catalog. The Brass Relay,
Test Playground and its demos, and Monitoring own their UI, schema and local
source dependencies in `apps/<id>/`. Named builders compile
to typed FlexBuffers; clients do not parse JSON UI. Playground loads a bounded
Wasm reducer. Other controls use declared setters rather than an unnecessary
executable module. See [the app guide](../apps/README.md).

The authority persists one Merkle root per app and acknowledges accepted
changes after storage succeeds. Public visitors share these roots with the
frame. Clients hold replicas, previews and pending commands in RAM only. Code,
fonts and blobs use disposable bounded caches: about 512 MiB web. The native
agent defaults to 2 GiB; the checked-in frame service configures 128 MiB.
Touch trails are an ephemeral stream and fade after five seconds.

Input Lab text is shared typed-register state; focus, selection, keyboard page
and repeat timers stay local. This is not character-by-character collaborative
text editing. Animation and 3D have independent intent roots. Their monotonic
clock, phase, meshes, glyph cache and presentation thread remain local; a CRDT
publication does not restart animation. The load control exercises real isolated
RAM CRDT work, without filling the public durable document with benchmark data.

Monitoring is private. Its admitted UI is withheld until authentication; the
existing seven-day Prometheus projection travels as a replaceable, read-only
binary aggregate view. Missing samples keep their gaps and original time
positions. Feed refreshes do not append weekly chart bytes to CRDT history.

## Local development

Use pinned Rust 1.90.0 and wasm-bindgen-cli 0.2.129. Fetch locked dependencies once.

```sh
bash scripts/build-dashboard-web.sh
cargo run -p otrios-dashboard --features canvas,server --bin canvas-server -- \
  --bind 127.0.0.1:8894 --apps apps --app-state target/app-state
```

Open `http://127.0.0.1:8894/`. The development binary is a thin entry into the
same authority used in production; its loopback listener is a trusted developer
endpoint. For public policy testing, configure a separate `--public-bind`, exact
`--origin`, and private `--auth-file` as described in [public access](PUBLIC_ACCESS.md).
App package output and browser bindings are generated and ignored by Git.

On Linux with DRM, GBM, EGL and GLES2 development libraries:

```sh
cargo build -p otrios-dashboard --features direct --bin dashboard-direct
```

The real frame uses its accepted rotation, touch calibration and exclusive
DRM ownership. Follow [direct display](DIRECT_DISPLAY.md); do not change kernel,
display timings or calibration as part of an app edit. Without `--sync`, a
local native experiment can load admitted packages from `--apps`; installed
frame releases use the negotiated `/session` connection.

## Validation and limits

`bash scripts/check.sh` runs formatting, strict Clippy, core and real-process
app/policy tests, plus signing and release-cache checks. All-feature Clippy and
`cargo test -p otrios-dashboard --features direct --bin dashboard-direct` also
check native worker isolation, transport/reconnect and input backpressure.
Linux CI exercises actual GPU output under Mesa; deployment acceptance also
runs the ARM host on the frame. Desktop browser results do not establish iPad
or physical touch performance.

This remains a bounded research runtime. Current fields are normalized integer
registers or bounded typed text/color/series values within a bounded Merkle
hierarchy. Canvas documents and messages are bounded at 512 KiB; checkpoints
rotate at 4 MiB. Qualified collection views and transfers are bounded at 1 MiB,
with up to 512 bytes of event-envelope overhead. Viewport-scoped collection
windows provide bounded ordered selections, including Brass Relay messages; general cross-authority
collection queries remain future work. Read-only monitoring aggregates and touch
trails avoid durable document history. General Unicode shaping, arbitrary native
module sandboxing and shared-memory OS IPC remain outside this migration.
[Modern app pattern](MODERN_APP_PATTERN.md) records the implementation and
remaining simplifications.
