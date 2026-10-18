# Dashboard validation checkpoint — 2026-10-18

## Completed

- Rust 1.90.0: `bash scripts/check.sh` passes: 12 core unit tests, three real child-process HTTP/WebSocket integration tests, and five OpenSSL signing-boundary tests. Formatting and strict Clippy pass. `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` also passes with SDL2 installed.
- Real-process tests cover durable restart, invalid/oversized documents, browser origin rejection including WebSocket upgrades, incremental push without HTTP polling, concurrent offline edits/reconnect, live release notices, retained old web assets and unchanged persisted state across code publication. Crypto tests use real Ed25519 signatures and reject tampered native bytes/manifests and a wrong publisher key; their dummy executables are never run.
- Real Rust/Wasm browser module loaded through private Tailscale Serve HTTPS. No public Funnel was enabled. The central room remains loopback-only behind Serve. It persists accepted history before acknowledging it.
- Native ARMv7 hard-float code runs on the actual RK3126 frame, Debian 13, existing Weston/Mesa/Lima, reporting `video=wayland renderer=opengles2`. The old Cog browser is stopped; the dashboard service is enabled at boot. No frame browser renders this app. The native service runs as `kiosk`; its code is verified against the pinned Ed25519 publisher before execution.
- Actual browser and frame both moved from release `4b17fecb1e4d6af634e5a1b10132ce969785e187557fced216720acfe0cfc12c` to `10d8746cf48cdd66be481fae4cd3628acd2b0f2fa514f407150a7e8b6c96a248` following a compositor header change to “LIVE SPACES”. Both retained snapshot `6c958f179e4f68cdae851788a91d164b4542dc84a06c2d42b9da00a12ef1a09f`. This was automatic browser reload plus signed native process replacement, not an in-process module swap.
- A subsequent browser touch reached the central room and frame at snapshot `54160232329a91450dfcf939013dff3a1fbc2946b838df65f2ce58df1a1d377e`. Restarting the real central room recovered that snapshot, and both hosts reconnected. Restarting the native systemd service recovered the same snapshot and resumed its TLS state stream (`stream_ready=true`). The recovery hook was corrected and rechecked so a healthy restart does not start a conflicting old-browser job.
- The native TLS client explicitly installs the Ring cryptography provider; the deployed HTTPS path was tested on both Mac OpenGL and the frame GLES2 host. A new native candidate must draw with GLES2 and receive a valid CRDT stream message before activation. Graphics startup alone does not qualify a new release.
- Actual frame GPU framebuffer readback was inspected at 1280×800: “LIVE SPACES”, Focus OFF, 5 touches and the matching sample graph. Desktop HTTPS browser and native framebuffer captures are retained in the local task outputs. This is not a photograph of the physical panel.
- Mac login services keep the central room and paired-build publisher running. An authorized local source feed triggers rebuilds; a configured trusted public Git branch supplies updates when the local feed is absent. A complete frozen source builds both artifacts before signing/advancing the release. Failed/stale builds preserve the current code.

Seed snapshot ID remains `d101fac0c9bc309dd8067bb52ceac20001c7c4dce513fe8e7c93820f623f4daf`.

## Packaging observations

The deployed normal release build produces a 2,446,885-byte Wasm module and a 5,570,236-byte unstripped ARMv7 native executable (including the native TLS stack). The private SDL2 library is 1,229,424 bytes and libXss 9,636 bytes, with notices installed alongside them. These are packaging sizes, not resident-memory or performance measurements, and are not comparable with earlier tiny transport kernels. Native uses existing system graphics/input libraries in addition to those files.

## Remaining acceptance and boundaries

The actual iPad still needs to open the private URL with Tailscale connected. The frame's physical layout/touch alignment, one-touch/one-update behavior and panel colour require someone at the device. Browser→frame network convergence is demonstrated; physical frame touch→browser is not yet confirmed. GPU readback does not settle the existing panel colour issue.

Presented FPS, end-to-end touch latency, idle/active CPU/memory, direct KMS/GBM and long-run history growth remain unmeasured for this app. Native networking uses a bounded background worker, but the in-process UI/worker bridge still exchanges full local documents; wire transport is incremental. Old releases and CRDT history are retained without garbage collection. State schema migration, arbitrary module streaming, capability enforcement and shared memfd IPC remain deferred.

The frame retains its old kiosk and recovery command; no kernel, bootloader, touch transform, browser profile, SSH forwarding policy or unrelated private source was changed. Automatic fallback is implemented and cryptographic rejection is tested; a deliberately crashing signed graphics release has not been deployed as a rollback test. The Mac must stay awake, online and logged in; this deployment has no cloud failover.

Linux CI builds Wasm and presents a native GLES2 frame under Mesa/Xvfb. CI pass/failure is reported on the pull request. That API-path check does not measure physical Mali performance. Full setup and recovery: [DASHBOARD_LIVE.md](../DASHBOARD_LIVE.md).
