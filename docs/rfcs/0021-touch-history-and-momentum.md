# RFC 0021: Timestamped touch history, lift-off and repeated strokes

Status: implemented and qualified on the follow-up branch, 2026-10-27. This
completes the input follow-up noted in RFC 0020. Deployment is separate.

## Behavior

All captured drags use the shared LGPL `dashboard::touch` trajectory estimator.
Scroll fields, the two-axis page and the long-document reader also share one
kinetic model. Sliders, the circular dial and the touch pad receive a filtered
release position; their values remain directly positioned rather than coasting.
Buttons retain their existing press/release hit-test rules.

Keep up to 200 ms of source-timestamped contact history, bounded at 64 samples.
A 4 ms bucket prevents high-rate devices from crowding out the useful history.
Positions follow the contact directly during dragging. Release estimates use
uniformly spaced time samples, not a count-weighted average of device reports.
A small f64 quadratic fit, after subtracting the coordinate origin and scaling
time, estimates position, velocity and acceleration. The last 16 ms residual
is box-filtered against the preceding trajectory; on a slower device, exclude
the complete final raw sampling interval from that prefix fit too. This keeps
interpolation of a bad last sample out of the supposedly trusted prefix.

The Up coordinate never supplies a velocity impulse. Its position correction is
bounded against the estimated trajectory. A stationary tail stops; an 80 ms
sampling gap suppresses release momentum, with attenuation beginning after
24 ms. A short reverse tail cannot launch in the older direction. Insufficient
history uses a conservative whole-contact average for scrolling and preserves
ordinary direct control delivery; it never divides by a forced 1 ms interval.
The estimate is a bounded model of observed input, not a claim of exact physical
finger motion. Captured traces can be used to tune it against actual panel noise.

Touch-down pauses coasting and retains its velocity as a candidate for the next
stroke. Sample that candidate analytically at the source touch-down time, with
a displacement correction if the presentation clock has already advanced.
This prevents delivery batching from changing the energy of repeated strokes.
On release, add the remaining decayed momentum only for a fresh same-direction
stroke lasting at most 250 ms, travelling at least 24 logical pixels, and moving
at least 0.15 px/ms and one quarter of the retained coast speed. More than 6 pixels
of reversal discards the carry; small contact jitter does not. Taps, cancellation,
caret following, navigation and hard stops discard momentum. Dragging remains
one-to-one, including immediate reversal away from a hard stop.

Single-stroke launch remains capped at 4,000 logical px/s. Repeated strokes can
reach 12,000 px/s per scrolling axis. Exponential drag remains 0.006/ms and stops
below 0.01 px/ms. Repeated strikes are therefore able to exceed 100% velocity,
with a finite cap and explicit braking behavior.

## Source timing and lifetime

Native evdev reports retain their sample age through the C/Rust boundary and
use `Instant` through the display-to-worker queue. Request the monotonic clock
on that input descriptor; the fallback computes age in its realtime clock.
Clock selection is descriptor-local; calibration is never rewritten. The high
level client accepts sample time separately from its dispatch/animation clock.
State publication clocks and wire protocols remain independent of pointer time.

The browser preserves coalesced events and batches up to 64 samples/200 ms per
pointer. Convert source timestamps through absolute time to the worker's time
origin, rather than replacing them with its processing time. Resize, cancellation
and privacy clearing discard pending input. This follows the chronological
coalesced-sample contract in [Pointer Events 3](https://www.w3.org/TR/pointerevents3/#coalesced-events).
Captured drag histories are mount/contact-local and bounded; they are not durable
app state, replicated state or an always-on input log.

The existing owned FFI boundary gains only sample age. No new unsafe module,
SIMD variant, graphics encoding or state wire format is introduced. The existing
pinned `serde_json` crate is reused as a **core dev-dependency** to load the JSON
recording fixtures; no new package or production core dependency is added.

## Capture and deterministic replay

`otrios-touch-trace-v1` records source timing, contact ID/phase, positions,
viewport, screen and control label. Native recordings keep raw normalized device
coordinates with their calibration/rotation mapping; replay uses the same Rust
mapping as the display. Admission bounds bytes, duration, sample count, contacts,
positions and phase order. A truncated contact ends with an explicit Cancel.
Replay checks the mounted route and uses disposable RAM-only state.

Keep the desired app/control visible, then record through the established
pinned SSH configuration:

```sh
python3 scripts/capture-touch-trace.py --config /PRIVATE/config.json \
  --screen test-playground/components --control 'Rotary dial' --seconds 15 \
  --output target/touch-input/my-dial.json
```

The recorder reads without grabbing, injecting input or stopping a service. Its
output contains no app text, credentials or canonical state. It currently records
the frame's single-contact ABS_X/ABS_Y/BTN_TOUCH path. Record the corresponding
viewport/surface arguments when those differ from 1280×800. Coordinates belong
to that layout; recapture after changing the UI geometry.

Build local packages, then replay without touching a display or authority:

```sh
cargo run -p otrios-dashboard --features canvas --bin canvas-pack --locked
cargo run -p otrios-dashboard --features canvas,host-sync \
  --example touch_replay --release --locked -- \
  target/touch-input/my-dial.json apps 20
```

The driver runs fresh clients with immediate, 16 ms and 64 ms delivery. It reports
CPU p50/p95, release velocities, final scroll positions/control fields and control
bounds. CPU timings include client ticks and the coast, exclude package admission
and initial document measurement, and are not rendering FPS. Playback does not
inject events into the live frame or change canonical app state.

Seven committed fixtures cover noisy slow scrolling, a paused release, repeated
vertical strokes, slider/dial/pad releases, and a captured synthetic evdev stroke
sequence. Analytic tests also cover acceleration, irregular/duplicate/old times,
8–33 ms sampling cadences, reversals, cancellation, hard bounds and reaching the
momentum cap. Original reader and renderer baselines remain preserved.

## Qualification

[Raw results and binary/source hashes](../experiments/touch-input-2026-10-27.json).

- The slow fixture moves at 80 px/s, then has a 34-pixel last-millisecond jump.
  The old last-difference rule in commit `7ccd7f3` would clamp that jump to
  4,000 px/s. The measured new launch is about 80 px/s. This old value is derived
  from the prior rule, not a newly measured old-binary trial.
- Five 80 ms strokes produce about 8,424 px/s at the fifth release, versus
  4,000 px/s for the first. Immediate/16/64 ms replay produces identical launches
  and final positions. A rapid-strike unit case reaches the 12,000 px/s cap.
- The passive capture contains 240 evdev samples and 20 complete synthetic
  strokes over 3.747 seconds. It is a recorded injection test, not a recording of
  a person's finger. Native/desktop replay verifies it with its original timing
  and calibration, including delayed delivery.
- Final native full-app trial: 48.93 kernel-presented fps over its 8.685-second
  active measurement window after warm-up, within a 16-second run, with no loading
  samples and greater-than-single-stroke velocity. The readback stall is included.
  This is a faster repeated-stroke workload, not the reader RFC's 4,000 px/s replay
  or a 60 fps lock. Production kiosk/calibration are restored after the trial.
- Actual Chrome replays the controls and scroll fixtures. Slider/pad/dial finish
  at their expected values; slow horizontal travel stays below 45 pixels, and
  repeated document strokes travel about 3,486 pixels. CDP can deliver late, so
  settled travel is the browser gate; an instantaneous velocity read can miss an
  already-decayed launch. Core replay checks the exact launch speeds separately.
- Browser document acceptance also verifies source anchors, navigation retirement
  and three real context loss/restoration cycles. Stress testing exposed a stale
  pre-loss packet referencing retired resources; a graphics generation now fences
  those packets and ACKs them without admitting their stale draw list. Idle input
  ticks preserve pending document reflow anchors.
- `bash scripts/check.sh`, all-target/all-feature ARM Clippy, native worker tests,
  web build and the JavaScript suites are required gates; the data file records
  their final results. No special landscape/panel correction is selected.
