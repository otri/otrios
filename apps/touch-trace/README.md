<!-- SPDX-License-Identifier: Apache-2.0 -->
# Touch Tracing

A full-screen shared drawing under **Test Playground → Touch Tracing**. The square
grid covers the viewport, and strokes share world coordinates across Frame,
iPhone and desktop browsers. The initial reference view is 1280×800. Camera
position and uniform zoom stay local, so another user's view never moves yours.
**Fit Drawing** fits retained stroke bounds, or restores the empty reference view.
**Clear** clears the shared drawing; **Exit** returns to Test Playground.

One finger draws; two fingers pan and pinch. Mouse left drag draws; right/middle
or Space drag pans. Wheel behavior follows the authenticated profile's Developer
Mode **Scroll: Pan / Zoom** switch. Pinch/Ctrl-wheel zooms at the pointer in either
mode. Safari trackpad gesture events are supported without doubling touch input.
The switch is hidden with Developer Mode off; no persistent input-mode selector
is added to the app toolbar.

The signed app uses generic `InkOptions.drawing`, `Ink`, `ClearInk` and `InkView`
contracts. Strokes are durable Space-owned app data inside the existing Merkle
root, journal and checkpoint, with normal Edit permissions. The optional drawing
branch does not change the app's existing empty scalar schema or dataset identity.
Each segment has its own identity and leaf; concurrent writers merge. Clear fences
late packets from the old drawing. Retention is bounded to 64 segments of up to
192 points, with connected endpoints on long strokes. The oldest segments are
evicted at capacity. Portable drawing export is explicitly unsupported for now.

This does not change device calibration. Legacy local retained ink and the
Components pad's five-second shared ghost trails remain independent modes.
See [RFC 0039](../../docs/rfcs/0039-shared-drawing.md).
