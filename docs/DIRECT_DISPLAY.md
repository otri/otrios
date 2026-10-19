<!-- SPDX-License-Identifier: Apache-2.0 -->
# Direct Linux display host

The photo frame runs one native OTriOS renderer on Linux DRM/KMS, GBM, EGL and
OpenGL ES 2. It owns scanout and reads calibrated Linux touch input. It does not
run the dashboard in a browser or through SDL/Wayland. Linux retains the hardware,
driver and scheduling base; this is a userspace runtime, not a replacement kernel.

`otrios-linux-display` contains the accepted narrow graphics FFI boundary. Common
Canvas state, input routing, scene resolution and host coordination remain safe
Rust. The display owner manages DRM setup, retained GPU buffers, font coverage
textures, swap/page-flip lifetime, touch and cleanup. Browser WebGL consumes the
same primitive geometry and font resource through its own small adapter.

The accepted frame configuration uses a 1280×800 logical scene, portrait scanout
rotated 90 degrees, verified color bits/stride and the existing calibrated touch
matrix. RGB565 and XRGB8888 remain explicit scanout choices. Coverage textures and
other GPU resources have their own formats and budgets; a 16-bit scanout does not
imply every texture is RGB565.

The direct service stops conflicting display owners and enters the console mode
through `frame-console-mode.py`. Its supervisor runs one verified direct executable,
checks actual first presentation, rotation, touch and framebuffer metadata, and
waits for confirmed Canvas readiness. Updates close the old renderer before
starting the candidate. Failure rolls back to a verified direct last-good release.
The accepted calibration bytes are read, never rewritten during code deployment.

Current build:

```sh
cargo build -p otrios-dashboard --features direct --bin dashboard-direct --release
```

The maintained service definition is
`tools/dashboard/otrios-dashboard-direct.service`. Deploy it with its matching
`frame-dashboard-agent.py`; there is no video-driver selection or SDL recovery
executable. The older OTriOS Wayland service/trial runners are removed. Conflicts
with their historical unit name remain a guard against accidental double ownership.

For isolated renderer work, `scripts/benchmark-dashboard-gles.py` accepts an
explicitly hashed development executable plus an explicit admitted `--apps` folder,
and restores prior service ownership.
Native reports count kernel page-flip completions separately from compose/present
call timings. `scripts/drm-flip-events.py` only parses captured DRM timestamps and
opens no device. Earlier SDL/Weston measurements remain historical evidence in
[the display experiments](experiments/direct-display-validation.md).
