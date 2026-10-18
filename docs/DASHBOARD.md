# Shared dashboard experiment

The active product experiment is a touch dashboard on an RK3126 photo frame and in a web browser. The frame uses a **native OpenGL ES 2 renderer**, with no browser or WebKit process. A Rust layout compositor creates a triangle list from structured immutable view state. Native SDL2/GLES2 and browser WebGL hosts draw that geometry. SDL2 provides display/input integration; it does not rasterize this UI in software. Linux refuses to start if the requested `opengles2` renderer is unavailable.

The user requested this experiment after the original object-store scaffold. It runs alongside the P1–P5 research plan rather than claiming that the planned sealed-memfd runtime is implemented. Runtime code and tests live in the LGPL core; host adapters, contracts and build tooling remain Apache-2.0. Distributed combined binaries still include the covered core and its licensing obligations.

## Data flow

1. Every replica starts from an identical Automerge seed change, then uses a fresh random actor. A loaded tab/process also gets a fresh actor. The seed actor is reserved and never reused for edits.
2. A focus-switch touch updates a boolean register, increments a CRDT counter and appends a mock signal sample in one change. Independent offline edits merge. Concurrent booleans use Automerge's deterministic register winner; they do not implement a last-wall-clock-write policy. The counter retains both concurrent increments, and the list retains both samples.
3. The projector sorts CRDT heads and builds a complete FlexBuffers view snapshot: label, switch, graph and touch count. It validates the candidate before replacing the active root. Cloning a snapshot pins its old bytes. Failed loads, invalid fields, unsupported schemas and oversized updates retain the previous root.
4. The compositor reads only the immutable projected view, produces triangles, and routes touches using the published node bounds. Hosts repaint on changes/exposure, with no idle animation loop. The graph is explicitly sample data, not telemetry.
5. The central room persists accepted Automerge history before acknowledging it and pushes incremental Automerge sync messages over `/stream` WebSocket connections. Each connection has its own sync session; reconnect negotiates missing changes, including offline edits. Native networking runs off the UI thread with bounded queues. The older `/sync` full-document POST remains for the baseline and diagnostics.
6. A release publisher builds the Wasm browser module and ARMv7 native executable from the same frozen source. It signs a manifest binding both artifacts, then atomically advances a release pointer. Existing streams announce that release. The browser saves its document and reloads the immutable web entry; the frame verifies its pinned Ed25519 publisher signature and artifact hash before gracefully replacing its native process. See [live deployment](DASHBOARD_LIVE.md).

The central room is the durable rendezvous for accepted state. Replicas can edit offline; reconnect merges their histories. It is not an exclusive-writer or globally linearizable authority. This proves a local complete view and eventual convergence. It does not promise a globally simultaneous snapshot across devices or atomically updated independent aggregates.

## Snapshot contract

FlexBuffers is a schema-less sibling of FlatBuffers. This experiment uses its safe builder/reader API with a separately specified v1 contract, avoiding handwritten unsafe code and generated unsafe accessors. It is **not the final unified IDL**. It does not provide CRDT semantics, atomic publication, memory ownership or authentication. A schema-based FlatBuffers comparison remains open.

The root map contains `schema = "otrios-view-v1"`, `heads` (sorted CRDT change hashes) and `nodes`. Each node has a unique `id`, `[x,y,width,height]` bounds in a 1280×800 canvas, a `label`, and `kind`: 0 label, 1 switch (boolean `enabled`), 2 graph (`samples`, integers 0–100). Minimum geometry bounds prevent underflow in the renderer. V1 readers ignore unknown fields; unknown node kinds and versions fail. The bounded reader validates all fields it consumes before activation and retains an owned typed projection beside the immutable bytes. This is not a zero-allocation or mmap implementation.

Snapshot IDs are SHA-256 of `OTriOS\0view\0v1\0` followed by exact snapshot bytes. The pinned projector produces identical bytes from identical heads/state. FlexBuffers as a format is not universally canonical; other encoders need not produce the same hash. IDs prove byte identity, not authority. The golden seed ID is `d101fac0c9bc309dd8067bb52ceac20001c7c4dce513fe8e7c93820f623f4daf`.

Limits: 2 MiB saved document, 32 KiB snapshot, 16 nodes, 64 heads, 96-byte labels, 32 displayed graph samples, 4096 retained demo samples. History is not compacted; a full room eventually needs a deliberate reset/migration. These caps do not make compressed Automerge input safe for hostile multi-tenant use. Browser state survives reload in session storage; closing the tab ends that local persistence. Native `--state` and server `--state` are optional local files. Server persistence uses a flushed temporary file and atomic rename before acknowledging an update. This is one local state file, without replicated backups, storage-failure recovery or a database availability guarantee.

Large immutable payloads such as photos should become separate content-addressed blobs referenced by a view snapshot, with activation gated on availability. Photo loading, text editing, a general scene-graph CRDT and blob transport are not implemented in this first slice.

## Build and run

Use the pinned Rust 1.90.0 toolchain and fetch dependencies once with `bash scripts/cloud-install.sh`. On Linux, native development needs SDL2 2.0.18+ development libraries (Debian: `libsdl2-dev`) and the system's EGL/GLES2 driver stack. On macOS with Homebrew SDL2, set `LIBRARY_PATH=/opt/homebrew/lib` for native builds if the linker cannot find SDL2. The Mac host requests desktop OpenGL; Linux requests GLES2. `SDL_RENDER_DRIVER` can explicitly choose another GPU renderer for experiments; software fallback is rejected by the normal Linux default.

Browser build:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
bash scripts/build-dashboard-web.sh
cargo run --release -p otrios-dashboard --features server --bin dashboard-server -- --state dashboard-state.automerge
```

Open `http://127.0.0.1:8876/` in two tabs. Tap the Focus mode tile, or use Tab/Space on its accessible switch. Both should converge to the same `VIEW` ID. Build output under `web/dashboard/pkg/` is ignored, so build it before serving. Browser hosts need WebAssembly, WebGL1 and cryptographic randomness. No iPhone validation is claimed yet.

Native host:

```sh
cargo run --release -p otrios-dashboard --features native --bin dashboard-native -- --sync http://127.0.0.1:8876/stream --state native-dashboard.automerge
```

Tap/click the switch; Space toggles it and Escape closes the host. `--fullscreen` selects a fullscreen window. `--frames 1` is a short GPU smoke test. `--seconds 60` bounds a trial; `--capture FILE.ppm` reads the actual GPU framebuffer for diagnosis. The startup log identifies the selected renderer; per-update logs identify snapshots, vertex counts and CPU submission time. Those timings are not presented FPS or end-to-end touch latency.

The server defaults to loopback and has no application-level accounts. The deployed development room uses private Tailscale Serve HTTPS in front of that listener, with an explicit `--origin https://HOST.TAILNET.ts.net` allowlist for browser POST/WebSocket origins. Tailnet access rules supply the network boundary; participating devices remain trusted editors. No public Funnel is used. The frame connects directly over WSS; it does not need SSH forwarding. Do not publish the loopback service directly to the public internet.

## ARMv7 build

```sh
bash scripts/build-dashboard-armv7.sh
```

The executable is under `target/dashboard-armv7/`. The script copies source into a temporary container and extracts the result, avoiding host-folder-sharing requirements. It also extracts private SDL2/libXss trial libraries and their copyright notices; verify the remaining dynamic dependency closure on the actual frame. It dynamically links the system SDL2 library. The Rust toolchain/base image are pinned; Debian package resolution still uses current Bookworm package repositories. This is not a bit-for-bit reproducible toolchain claim.

On the frame, first test the native GLES2 window on its existing Wayland compositor, preserving the working display rotation and calibrated touch. This still uses native graphics, without a browser. A subsequent direct KMS/GBM backend trial must handle display ownership, native 800×1280 rotation and the calibrated touch transform explicitly. The existing frame project reports much higher presented FPS in direct KMS than in Weston; this dashboard does not claim to reproduce that result yet. Do not apply kernel/display fixes as part of this app experiment.

## Checks and acceptance

`bash scripts/check.sh` runs formatting, strict Clippy, unit tests real child-process HTTP/WebSocket sync, persistence and code-release tests using temporary localhost ports, and OpenSSL signing-boundary tests. `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` additionally checks native adapters where SDL2 development files are available. Linux CI also builds Wasm and exercises a native GLES2 frame under Mesa/Xvfb; that establishes API compatibility, not Mali hardware performance.

Required hardware evidence: real renderer identity, no browser process during the trial, visible correct layout, physically aligned touch, one touch represented exactly once, browser↔frame convergence, recovery after disconnection, memory/CPU at idle and while touching, and rollback to the existing kiosk. Preserve the kernel, browser profile and recovery backups. A transient app trial is separate from persistent boot deployment.

For the existing SPD frame, `scripts/frame-dashboard-trial.sh --check STAGING_DIR` checks readiness without replacing the kiosk. Its explicit `--run` mode uses a transient service, stops only the browser, starts the native GLES2 host on existing Weston and restores the browser with `ExecStopPost` after exit/crash/timeout. Default duration is 60 seconds. It leaves no boot-enabled dashboard service. For that older bounded trial, pass `--sync` with an accessible room URL; SSH forwarding must already be configured if using a localhost tunnel. The active persistent deployment instead uses the signed updater and direct Tailscale HTTPS described in DASHBOARD_LIVE.md. Physical layout, touch and colour confirmation still require someone at the frame.

Dependencies: Automerge 0.12.0 (MIT) for a reference CRDT; FlexBuffers 25.12.19 (Apache-2.0) for binary views; SHA-2 0.11.1 (MIT/Apache-2.0) for standard hashes; font8x8 0.3.1 (MIT) for a tiny deterministic demo font; wasm-bindgen 0.2.129 (MIT/Apache-2.0) for the browser boundary; SDL2 Rust bindings 0.38.0 (MIT) for native GPU/input; Axum 0.8.9/Tokio 1.53.3 for the async room; tungstenite 0.30.0 for native/test WebSockets; rustls 0.23.46 with the explicitly selected Ring provider and WebPKI roots for native HTTPS; futures-util and serde_json for host orchestration. These host dependencies replace tiny_http/ureq to support persistent streams. Rustls uses its Apache-2.0/ISC/MIT grant; consult Cargo metadata and Ring notices for transitive terms. System SDL2 retains its zlib license. Cargo.lock records transitive versions; none of these additions selects a stable platform ABI.
