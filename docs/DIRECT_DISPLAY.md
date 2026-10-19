<!-- SPDX-License-Identifier: Apache-2.0 -->
# Direct photo-frame display

The ARMv7 dashboard can own the panel directly: shared Rust scene/presentation → retained GLES2 VBOs → EGL/GBM → Linux DRM/KMS → DSI panel. Linux, Mesa Lima and the existing drivers remain. Weston and the old browser are stopped while this host owns the display. Browser Wasm/WebGL continues to use the same scene and CRDT protocol.

## Coordinates and touch

The frame scans out 800×1280. `--rotation 90` projects the 1280×800 logical canvas into that physical surface; static geometry, animated geometry, cached regions and mouse input use the same reversible transform. The direct host rejects an unexpected output size instead of silently letterboxing.

The SDL-free `dashboard-direct` host reads the explicitly configured evdev device (`OTRIOS_TOUCH_DEVICE`; frame profile defaults to its stable by-path name). ABS_X/ABS_Y and BTN_TOUCH provide normalized primary-contact events; SYN_DROPPED cancels capture and suppresses input until release. The SDL recovery host retains its existing evdev implementation. The supervisor reads the accepted `LIBINPUT_CALIBRATION_MATRIX` from `/etc/udev/rules.d/99-spd-touch.rules`, validates it, and supplies `--touch-matrix` to the host. The matrix maps raw input into physical panel coordinates; inverse surface rotation produces logical responder coordinates. The rule is never edited. Touch-to-mouse emulation is disabled on KMS to avoid duplicate presses. The first contact remains captured until release/cancel; additional fingers do not steal it. Without valid calibration a direct candidate cannot pass readiness.

This is a board-specific deployment profile, not universal panel detection. The matrix must come from the actual accepted device configuration. Readiness requires a discovered touch device as well as loaded calibration. The direct unit permits `AF_NETLINK` for SDL/udev discovery; omitting it allowed rendering but left SDL with no open touch device. The user confirmed touch works after that correction. The SDL-free host creates no cursor and explicitly disables the legacy KMS cursor plane.

## Exclusive display service and recovery

Install `scripts/frame-dashboard-agent.py` and `scripts/frame-console-mode.py` under `/opt/otrios-dashboard`, and `tools/dashboard/otrios-dashboard-direct.service` under `/etc/systemd/system`. Preserve the installed agent as a recovery copy before replacement. The existing publisher public key, signed cache, HTTPS configuration and `kiosk` video/render/input groups are reused.

The direct unit conflicts with Weston, the normal dashboard and the browser. It saves the console mode in its private runtime directory, enters graphics mode, and restores the saved mode on stop/failure. It accepts signed updates only after GLES2 drawing, the expected KMS output/rotation/calibration, and a live state stream. Candidate failure retains the signed rollback path. Complete direct-service failure starts the existing Wayland dashboard recovery service.

```sh
sudo systemctl daemon-reload
sudo systemctl start otrios-dashboard-direct.service
# Return to the existing dashboard profile:
sudo systemctl stop otrios-dashboard-direct.service
sudo systemctl start otrios-dashboard.service
```

The direct unit is installed but **not enabled at boot**; the existing boot recovery remains. A deliberate successful stop does not automatically start another renderer. Do not launch a standalone benchmark alongside an active dashboard. The benchmark controller handles the handoff and restores the original services.

## Measuring the renderer

For the component playground's input/CRDT/scene CPU path, the read-only
`dashboard-profile` probe and paired frame results are documented in
[component playground validation](experiments/component-playground-validation.md).

On the frame, as root:

```sh
python3 scripts/benchmark-dashboard-direct.py --direct-only \
  --output /var/lib/otrios-dashboard/direct-results --seconds 20
```

Omit `--direct-only` for the paired Weston/KMS comparison. Every case runs alone, with no `--state` or `--sync`, and therefore does not mutate the live room. The controller signature-verifies the cached executable, holds an exclusive trial lock, bounds child lifetime, and restores the original service/console state on normal exit or handled interruption. A hard power loss or SIGKILL cannot run Python cleanup; the unchanged boot profile is the recovery path.

`--trace-tool /path/to/strace` adds a separate 12-second kernel page-flip sample, filtering reads to `/dev/dri/card0`. The parser counts `DRM_EVENT_FLIP_COMPLETE` records, skips two seconds of warm-up, and reports actual completion intervals and missing vblank sequences. Ordinary reports measure host frame-start cadence and CPU submission/present-call time. Neither measures photons, touch latency, or worst-case frame delivery. Tracing overhead is kept separate from the ordinary runs. Raw traces stay private on the frame; aggregate results contain no raw input or network data.

The current mode requests a 66 MHz pixel clock with totals 880×1324: about 56.6 Hz, or 17.7 ms per refresh. A steady 60 FPS cannot be displayed in this mode. At unchanged totals, 60 Hz would need a 69.9072 MHz pixel clock; that arithmetic does not establish panel, PLL or DSI tolerance. Historical porting evidence records clock rounding and DSI timing issues, so this renderer experiment changes no hardware clocks, panel timings, kernel, bootloader or calibration.

For less intrusive feedback than per-syscall tracing, build `tools/dashboard/flip-audit-armv7.c` with the ARM hard-float compiler and use `--flip-audit /absolute/path/to/flip-audit.so`. This optional diagnostic targets ARM32 glibc/libdrm and counts completion records in existing libdrm reads; it makes no per-frame log writes. It is used only by the bounded trial, never the normal service. It does not alter buffer callbacks or replace installed libraries. The controller records missing/incompatible event data as a diagnostic rather than inventing presentation results.

```sh
arm-linux-gnueabihf-gcc -shared -fPIC -O2 -Wall -Wextra -Werror \
  tools/dashboard/flip-audit-armv7.c -o flip-audit.so
```

Use `--scenario cubes-12-idle` (repeatable) to select cases, `--release DIGEST` for a specific signed cached executable, and `--renderer-batching 0` or `1` for a controlled submission comparison. The normal SDL host explicitly enables batching. `--completion-probe` adds a one-pixel forced completion/readback only to isolated benchmarks; its timing includes resolve/readback overhead and is not a pure GPU timer. The production host rejects this option when live state/sync is supplied.

## SDL-free host

[RFC 0001](rfcs/0001-direct-linux-gles2.md) was approved on 2026-10-19. `otrios-linux-display` owns DRM master, the matching GBM/EGL visual, a GLES2 shader/context, retained packed VBOs and the evdev descriptor. The main render thread never reconciles or persists CRDT state. A separate bounded presentation worker prepares telemetry text; app state retains its existing independent worker. Complete six-float triangles are checked before a single isolated Rust FFI module calls the C platform adapter. Core and other hosts retain the workspace unsafe-code ban.

Build `cargo build -p otrios-dashboard --features direct --bin dashboard-direct --release` on Linux with libdrm/GBM/EGL/GLES2 development headers. `OTRIOS_SCANOUT_FORMAT=rgb565` selects true 5/6/5 EGL rendering and 16-bit GBM/KMS allocation; `xrgb8888` selects 8/8/8/0 and 32-bit storage. The adapter checks the registered format/stride using `drmModeGetFB2` and reports kernel page-flip completions directly. It keeps the current scanout BO locked until the replacement completes, restores/disables the CRTC on cleanup, and then releases resources.

The builder accepts `native_host: "direct"` in its private config. It publishes **both** the direct executable and a signed SDL recovery executable under the existing manifest. The supervisor selects SDL for the Wayland recovery service and the direct primary for the direct service. Source fingerprints include C and header files, so changes to platform code trigger hot reload too. The direct service profile selects RGB565; it remains disabled at boot.

`benchmark-dashboard-gles.py --executable PATH --sha256 DIGEST --output DIRECTORY --seconds 20` compares both formats on an explicitly hashed development binary, with no live state/sync. It holds the same trial lock and restores service/console state. It reports actual page-flip feedback, not host-loop averages. See [measured results](experiments/direct-display-validation.md) for the full matrix, limits and retained historical SDL measurements.
