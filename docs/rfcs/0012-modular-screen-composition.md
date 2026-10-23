<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0012: Modular screen composition and local viewports

Status: implementation; browser acceptance is the first validation target.
Native ARM compilation and projection/touch tests are separate from physical
frame acceptance, which remains pending.

## Ownership and sizing

A component owns its local design size, primitive structure, style, behavior and
state bindings. A screen composition assigns outer frames to named component
instances. The host selects a local logical viewport. Browser and native use the
same bounded Rust resolver; viewport changes never become replicated app edits.

Layout v3 adds an optional `Composition` to the binary Blueprint. Wide and tall
plans contain rows/columns, gaps, cross-axis alignment and component references.
Each slot declares preferred/minimum dimensions and flexible axes. Flexible
slots share surplus equally and shrink in proportion to their available
preferred-to-minimum space. Fixed axes retain their authored size. Existing pins
and springs resolve the component's interior after allocation. All calculations
restart from authored geometry, without accumulated resize drift.

The first host profiles are 1280×800 landscape and 800×1280 portrait. Browsers
select a profile from the available window orientation and fit that logical
canvas uniformly into the window, independently of device pixel ratio. Native
selects `--layout landscape|portrait` independently of physical `--rotation` and
touch calibration. Unconverted v1/v2 UIs keep their 1280×800 logical scene; the
native surface fits these uniformly into its configured output. Browser canvases
use the actual extent returned with their scene, including legacy scenes.

Phone-specific typography, text wrapping, scrolling and general constraint
graphs are separate work. An infeasible composition is rejected while retaining
the prior scene and state. Supported minimum sizes are explicit.

## Authoring and identity

```rust
let linear = || place("linear", [370., 410.]);
let rotary = || place("rotary", [370., 410.]);
let pad = || place("pad", [400., 410.]).flex_width();
let wide = row(22., vec![linear(), rotary(), pad()]);
let tall = column(16., vec![row(12., vec![linear(), rotary()]), pad()]);
```

`.key("linear")` names an existing component instance for authoring. Both plans
compile references to the same tree, with the same node, responder, selector and
handler identities. Plans must reference the same set of immediate root
components exactly once. Keys are local to a compiled UI; they do not promise
identity preservation across arbitrary source edits or code replacement.
`.with_touch_pad("pad-control")` resolves a named responder through the compiler,
replacing assumptions about its numeric tree order.

Home demonstrates three/two columns. Playground arranges its same instrument
cards as a wide row or two cards above a full-width pad card. Layout Lab composes
reusable toggle, graph, labeled sliders and resizable cards. Components retain
their existing colors, primitive styling and state semantics.

## Runtime and input

The Canvas client retains its mount, pending intents, editor/focus state and
confirmed values when a viewport changes. A candidate composition is resolved
before committing it. Accepted relayout cancels contacts and flushes pending
edits without remounting state or restarting sessions. Browser orientation
changes also cancel host contacts for legacy scenes whose logical size stays
fixed, because the physical input mapping changes.

Browser frames carry their actual logical extent and a worker-local viewport
revision. Pixels, pointer mapping and semantic controls use that same extent.
Queued input from an earlier viewport revision is ignored. Semantic buttons
are recreated on app activation, preserving canonical focus order; orientation
changes retain the existing buttons. Glyph caches are invalidated when logical
projection changes. Native drawing and calibrated touch share the same
size-aware rotation and uniform-fit transform.

Ink retains existing normalized pad coordinates and the shared session. Layout
does not alter the CRDT or state transport: Canvas sync v4 and the stable
release-control v1 opening envelope remain unchanged. Old hosts reject layout
v3; matching code and UI must ship together through signed release activation.

## Bounds and acceptance

Each plan permits at most 128 entries and eight nested groups, with finite
bounded dimensions, insets and gaps. Referenced nodes must be immediate children
of the root template, without duplicate placement or variant-dependent identity.
Existing scene count/depth/geometry guards still apply.

Tests cover allocation, infeasible sizes, malformed references, immutable old
scenes, shared state across orientations, stable control bindings and capture
cancellation. Projection/touch round trips cover all four native output
rotations, including legacy scenes fitted into a portrait surface.

Run the normal repository checks and browser worker/GPU tests. For actual WebGL
acceptance against a disposable authority, run:

```sh
node scripts/test-responsive-browser.mjs --url http://127.0.0.1:PORT \
  --output target/responsive-browser
```

This requires Chromium and Node with built-in WebSocket support. It records six
screenshots, checks both compositions and shared edits/ink between two clients,
then resizes during a gesture and verifies subsequent input. Its disposable
browser profile is removed afterward. Software WebGL acceptance does not establish
physical frame touch, panel orientation or device GPU performance.
