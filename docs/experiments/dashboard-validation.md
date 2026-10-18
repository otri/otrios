# Dashboard validation checkpoint — 2026-10-18

## Completed

- Rust 1.90.0: `bash scripts/check.sh` passes (10 unit tests plus one child-process HTTP sync/persistence test). The process test uses real loopback connections, concurrent replica edits, invalid/oversized documents, cross-origin rejection and recovery after server restart. It required network permission in the Mac sandbox; it was rerun with that permission and passed.
- `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` passes, including native host code.
- `bash scripts/build-dashboard-web.sh` builds the real Rust/Wasm browser module with wasm-bindgen 0.2.129. It was loaded in the in-app browser, not replaced by a JavaScript state/compositor implementation.
- Two independent browser tabs: first touch changed Focus mode ON→OFF; the second tab loaded OFF. A touch in the second tab changed OFF→ON; both tabs converged to snapshot `afd545340a534840a22fb13af4eec7dff8178b75b139942a7f2dfea7ccba090f`. The Mac native host logged the same snapshot after polling. This is browser→native evidence; native→browser physical input has not been validated yet.
- Native Apple Silicon/Cocoa host selected `renderer=opengl`, submitted 20,022 vertices for the seed view and exited successfully with `--frames 1`. Its actual framebuffer was captured and visually checked. This is a native OpenGL test, not an emulated GLES2/Mali result. Submission timings include setup/capture differences and are not an FPS benchmark.
- ARMv7 hard-float executable compiled successfully in a separate Debian Bookworm/Rust 1.90.0 cross-builder. Existing photo-frame kernel/build sources were read only. The running frame reports Debian 13 armv7 and active Weston/kiosk services.
- The app and private Debian SDL2 2.26.5/libXss libraries were staged in a dedicated temporary folder. No packages were installed on the frame, and no kernel, boot, display calibration, browser profile or persistent service was changed. The trial wrapper's `--check` passed after dependency closure was qualified.

Seed snapshot ID, also observed in the Wasm browser and Mac native host:
`d101fac0c9bc309dd8067bb52ceac20001c7c4dce513fe8e7c93820f623f4daf`.

## Current size baseline

The initial normal release build (no LTO/size tuning) produced a 2,386,155-byte Wasm module, roughly 4.2 MB unstripped ARMv7 native executable, 1,229,424-byte private SDL2 library and 9,636-byte libXss helper. The Wasm file includes the CRDT, decoder, projector and compositor. Native additionally depends on existing system libraries. These numbers are packaging observations, not memory or performance results, and are not comparable to the earlier tiny arithmetic transport kernels.

## Pending hardware acceptance

The native app has not yet been physically accepted on the Rockchip display. The prepared 60-second trial uses native GLES2 on the existing Wayland compositor, with the browser stopped and automatically restored afterwards. Someone at the screen must confirm layout, one-touch/one-update behavior and alignment; verify browser↔frame convergence and idle/active CPU/memory. The existing frame's unresolved colour consistency remains separate. Presented FPS, touch latency, direct KMS/GBM, iPhone behavior, network interruption and long-run history growth still need measurement.

Linux CI includes the tests plus Wasm compilation and a native GLES2/Mesa/Xvfb smoke test. Its run status is reported on the pull request; CI configuration alone is not a passing result. A Mesa software GPU establishes the host API path, not the physical Mali driver's performance.
