<!-- SPDX-License-Identifier: Apache-2.0 -->
# Touch Tracing

A full-screen local ink surface under **Test Playground → Touch Tracing**.
Draw against the grid and edge border. Strokes stay visible until **Clear** or
leaving the app; **Exit** returns to Test Playground. This displays calibrated
input and does not change the touch device or its calibration.

The app is a signed declarative `.otapp`, using the reusable `TouchPad` ink
contract with `InkOptions`: a configurable lifetime (`None` retains strokes),
a bounded segment budget, and full-screen presentation. `Operation::Ink` and
`ClearInk` act on mount-local presentation data; this app declares no replicated
state fields. It sends no strokes to other clients and writes no drawing files.
Retention is bounded to 64 segments of at most 192 points each; adjacent segments
share an endpoint so long traces remain connected. Oldest segments are evicted
when that budget is exhausted. The existing shared Components pad retains its
five-second ghost-ink behavior.

Developer Mode and Hit Zone on Touch belong to the authenticated user's profile;
they are independent of this app and do not grant additional access.
