<!-- SPDX-License-Identifier: Apache-2.0 -->
# Mesa GPU test display lifetime race

The PR's CI runs [73](https://github.com/otri/otrios/actions/runs/39173433375)
and [77](https://github.com/otri/otrios/actions/runs/39174170823) failed in
`mesa_retained_geometry_and_alpha_pixels`, at surface presentation:
`Swap EGL surface (errno=2, EGL=0x3001)`. The Rust, Wasm, process, native worker
and display-build checks passed. Run 75 on the intervening code commit passed,
consistent with a scheduling-dependent failure.

The four ignored Mesa tests run concurrently under Rust's test harness. The
surfaceless platform request uses the same null native-display key and can return
the same process-wide EGLDisplay. Each test's owner calls eglInitialize and
ultimately eglTerminate; one owner's teardown can invalidate another test's live
surface. The production renderer uses one display owner.

The test module now holds one mutex for each Mesa test's complete lifetime,
including teardown. Guard poisoning is recovered so an assertion failure does not
hide the remaining tests behind a lock-poison failure. All pixel, font, geometry,
cache, rotation and color assertions remain active. The fix changes test code only;
it does not alter the renderer or disable GPU acceptance.

Local reproduction used a separate Debian Linux aarch64 container, Rust 1.90.0,
Mesa 22.3.6 and software rendering. The unchanged revision failed with EGL=0x3001
while looking up the RGB565 config (a different race point from GitHub's Ubuntu
x86_64 runner). The corrected suite passed five consecutive executions of all four
tests with `--test-threads=4`. The three non-ignored boundary tests, Linux package
Clippy with warnings denied, and `bash scripts/check.sh` also passed.

```sh
EGL_PLATFORM=surfaceless LIBGL_ALWAYS_SOFTWARE=1 \
  cargo test -p otrios-linux-display --features platform --locked -- --ignored --test-threads=4
```

The workflow triggers for both pushes and pull-request updates, so one branch push
can produce two CI runs. That duplication is separate from the EGL race; triggers
and assertions are unchanged by this fix.
