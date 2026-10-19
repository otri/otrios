<!-- SPDX-License-Identifier: Apache-2.0 -->
# Animation and 3D presentation

Animation Lab and 3D Lab are independent Canvas apps. Their declared state fields
supply playing, speed and object-count intent. The authority checks and persists
those assignments through the same bounded Merkle adapter as other apps. The
apps' UI/code packages are independent of their State references.

Each presentation owner holds a monotonic clock, phase, eased speed, frame samples,
meshes and GPU resources. A new immutable app view updates intent without resetting
phase. Animation does not write a state update every frame. Different devices
need not share a phase; synchronized timelines, keyframes and physics remain
future work.

The native app worker owns admission, replica, edits and static scene composition.
Its network thread owns the single negotiated socket and release metadata. The
presentation thread receives a bounded latest immutable view and keeps rendering
while state processing is delayed. Input/queue overload is explicit. Readiness is
reported after actual presentation; no CRDT merge, document fsync or readiness-file
write runs in the render loop. Only the direct DRM/GBM/EGL/GLES2 host is supported.

In the browser, the app worker owns RAM state/admission and the main-thread network
bootstrap starts before WASM. The main thread owns WebGL and animation. One view
can be in flight and one latest view pending; immutable resource caches are
separate from document state. There is no IndexedDB checkpoint or legacy Shell.

3D Lab transforms four vertices per regular tetrahedron in Rust, applies
perspective, culls hidden faces and painter-sorts the four triangular faces.
Objects do not intersect. Projected faces are validated clipped `Triangles`
primitives shared by both renderers. Arbitrary meshes, depth-buffered general
scenes, lighting and GPU transforms are outside this demo.

State load exercises a bounded isolated Merkle StateAdapter/Document pair in RAM,
reset after 256 mutations, rather than filling the public durable app root. The
stall mode deliberately pauses the app worker while presentation consumes its
last view. New inputs may wait during that pause; the frame clock continues.
The old Automerge-backed stress replicas and duplicate presentation edit overlay
are removed.

The HUD measures host cadence with at most 3,600 intervals. These are not photon
latency measurements. Native reports also contain actual kernel page-flip timing;
the direct experimental runner is `scripts/benchmark-dashboard-gles.py`. Earlier
SDL/Wayland and cube benchmarks remain historical reports, not supported hosts.
See [current measurements](experiments/canvas-packed-wire.md),
[display ownership](DIRECT_DISPLAY.md) and [resource limits](ARCHITECTURE-RESOURCES.md).
