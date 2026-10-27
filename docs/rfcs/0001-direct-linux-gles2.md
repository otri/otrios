<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0001: Own the Linux display and GLES2 renderer

Status: accepted by the user on 2026-10-19. The narrow graphics FFI boundary was explicitly approved in this chat.

The user requested direct display/OpenGL ES2 initialization, retained primitive rendering, measured panel-rate playback, and comparison of RGB565 with 32-bit colour. Implement one Linux backend that owns DRM/KMS → GBM → EGL → GLES2 and evdev input. Keep the current signed-update/state protocol and the working SDL host as recovery. No kernel, clock, panel-timing, bootloader or calibration changes belong to this work.

## Result and scope

A Linux `dashboard-direct` host uses the existing core Replica, Shell, responder chain, immutable view handoff and local Presenter. It renders the same triangles as the browser. Static UI and HUD geometry remain in GPU buffers until their contents change; only animated geometry uploads each frame. It does not copy a full cached UI image per frame. The first version needs no UI texture atlas or new scene/CRDT encoding.

Own display selection, matching EGL native visual, 16/32-bit scanout allocation, DRM framebuffer registration, GPU completion and page-flip feedback. Compare RGB565 and XRGB8888 at the current 800×1280 mode, using explicit format/stride/colour-bit evidence and image checks. A later image-texture API can independently choose opaque RGB565 or alpha-bearing RGBA8; RGB565 cannot retain alpha.

A named touch device supplies evdev events; retain the accepted calibration matrix and inverse surface rotation. Preserve responder capture/cancel, virtual keyboard behavior and local navigation. SYN_DROPPED cancels input and requires release before a new press. Do not create a hardware cursor.

## One narrow foreign-function boundary

Add an Apache-2.0 host/platform crate, `otrios-linux-display`. The Rust API exposes owned display/renderer objects and checked slices/events, without raw pointers. A small C adapter includes the platform's real DRM/GBM/EGL/GLES2/input headers, avoiding handwritten copies of large platform structs. No additional Rust dependency is required for this initial adapter.

This crate alone changes its unsafe lint from workspace `forbid` to crate-default `deny`, allowing unsafe only inside one documented FFI module. Every other crate, including all core runtime code and common host state logic, retains `unsafe_code = "forbid"`. The wrapper owns a non-null opaque adapter pointer, is neither Send nor Sync, and destroys it exactly once. Foreign calls consume Rust buffers synchronously; no foreign code retains a Rust allocation or callback closure.

Required invariants:

1. The EGL context, GL resources, DRM descriptor and GBM surface have one owner and one thread. Display/native window/context all belong to the same GBM platform/device.
2. Geometry has complete six-float vertices/three-vertex triangles; validate finite coordinates, RGBA bounds, counts, byte arithmetic and fixed allocation caps before crossing FFI. GPU uploads copy into owned buffers before returning.
3. Use owned packed GPU vertices and retained VBOs. Opaque groups disable blending; translucent groups preserve source-over order. Shader compilation, linking, buffer allocation and GL errors fail explicitly.
4. Every locked scanout BO and registered framebuffer has exactly one owner. Keep the currently displayed buffer locked until the replacement's kernel flip completion. Bound waits and handle EINTR; never recycle an uncertain displayed buffer.
5. Match GBM FourCC, EGL native visual, KMS framebuffer depth/stride and observed colour bits. Fail an unsupported 16-bit request rather than converting silently to 32-bit.
6. On shutdown/failure, restore or disable this CRTC before freeing scanout buffers, then remove framebuffers, release BOs, destroy GL/EGL objects/surfaces and GBM/device descriptors in dependency order. Restore the console through the existing root service hook.
7. Read only the configured input descriptor. Validate its axes, handle short/nonblocking reads and cancel on dropped events/device loss. No input payload or Rust pointer is retained across FFI calls.
8. Partial initialization uses the same cleanup path. Safe callers cannot call into a destroyed context or send the owner to another thread.

The adapter is host integration, not copied LGPL runtime implementation. Existing shared primitive/CRDT algorithms remain in `otrios-core` under LGPL-3.0-only. Preserve applicable third-party notices for any reused code.

## Delivery and acceptance

Refactor the existing safe state/network worker into a module shared by the SDL and direct executables. Keep bounded latest-view publication, off-thread reconciliation/persistence, signed rollback and browser protocol compatibility. Add direct-host readiness with actual backend, framebuffer colour bits, touch accessibility, rendered snapshot and live stream status. Publish the new executable only after compatibility checks; keep the previous signed executable available.

Check Rust formatting/strict Clippy, core and worker tests, C compilation with warnings as errors, bad geometry/resource limits, headless Mesa shader/pixel checks, partial-init cleanup, real KMS colour/rotation and physical touch. Measure kernel page-flip completions in the backend itself, alongside CPU/GPU work. Compare idle and loaded scenes with 250 ms state-worker stalls. Report missed vblanks and p95/p99; an FPS average alone is not evidence of refresh lock.

Success is sustained delivery at the existing approximately 56.6 Hz panel refresh with no missed vblanks in the accepted bounded runs, preserved live state/code updates and working touch/navigation. Literal 60 Hz needs a separately validated panel mode. Leave the existing boot recovery intact while testing.

## Canonical text styles within the existing boundary — 2026-10-27

The style update follows the same approved synchronous slice-copy boundary as
text viewports. Rust checks a maximum 4,096 finite nonnegative font em values;
C checks the count against its owned draw plan, validates overlapping ranges,
and builds an owned per-quad scale table before replacing the previous one.
Zero retains inference for older callers. Geometry replacement and destruction
free the table. No Rust pointer is retained and the owner stays on its EGL thread.
This extends the graphics adapter, not shared-memory IPC or the state protocol.

## Independent input ownership — 2026-10-28

The keyboard regression follow-up separates evdev ownership from graphics
ownership within this existing platform adapter. The display continues to own
DRM/GBM/EGL/GLES on one thread. A separate opaque input owner is created, read and
destroyed on the input thread; it never owns or calls EGL/GL. The existing evdev
axis validation, monotonic sample ages, initial-contact suppression and
SYN_DROPPED cancellation/release rules remain shared by both entry points.

The safe wrapper for that owner uses the same reviewed FFI invariants: !Send and
!Sync, synchronous bounded event slices, no callbacks or retained Rust pointers,
one destructor and complete cleanup on initialization failure. No unsafe lint
scope changes and no driver/ABI structure copies are introduced. The host uses a
bounded input queue and copies a small calibration/epoch/viewport target under a
short mutex; no GL/display owner crosses threads. Input collection and dispatch
must continue while graphics upload or a page flip is blocked. Event overload or
device failure must be explicit, and mount replacement/revocation must suppress
stale input rather than directing an old contact into a new UI.
