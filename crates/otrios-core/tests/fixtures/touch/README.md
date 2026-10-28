<!-- SPDX-License-Identifier: LGPL-3.0-only -->
# Touch replay fixtures

These bounded `otrios-touch-trace-v1` inputs are runtime test data. The six
`synthetic-golden` cases pin intentional paths and terminal noise independently
of the input estimator. `frame-repeat-capture.json` passively records synthetic
evdev injection on the frame, including real sample cadence and its accepted
calibration. It contains 20 complete strokes; it is not human finger data.

Run them through the shared runtime with the `touch_replay` example. See
[the capture/replay guide](../../../../../docs/rfcs/0021-touch-history-and-momentum.md).

Screen routes follow Test Playground. The three synthetic slider/dial/pad cases
are translated to their current brass-layout control bounds; their timing and
relative motion are unchanged. Captured frame sample positions and timestamps
are unchanged.
