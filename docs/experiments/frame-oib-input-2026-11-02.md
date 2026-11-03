<!-- SPDX-License-Identifier: Apache-2.0 -->
# Frame OIB input performance — 2026-11-02

Status: **fixed, signed and deployed**. Active release:
`802dba22acd4be013d2d21109bcab12f86368b1b59644610391bc429f8583831`. State protocol remains **v21**;
release-control v1, the signed OIB seed, app schemas and data bindings are unchanged.
This follows the [initial O1 acceptance](live-oib-2026-11-02.md).

## What caused the stalls

The native owner drained input until its queue became empty. At 30 Hz, the old
slider path could spend longer rebuilding a scene than the interval between
samples, postponing presentation until the gesture ended. A captured baseline
had a **3.09-second gap** between readiness publications during its drag.

Every native frame also formatted all inspector properties. Each property cloned
its UI, decoded the persisted descriptor and read every active property again.
On Frame that cost about 35 ms just for diagnostics. The painter retained text
but retessellated moving button geometry and uploaded a new panel mesh. A changed
slider percentage rebuilt the inspector as part of that same mesh.

## Current behavior

OIB display data is cached by the admitted OIB root and pending UI intent.
Property reads borrow their admitted UI, and one property-map read supplies all
inspector strings. Ordinary slider edits do not display “Saving UI change”.

All bounded retained primitives use local geometry plus draw translations and
ancestor clip ranges. Scrolling updates those ranges and semantic hit rectangles;
it does not tessellate or upload the Live UI panel again. The CPU still traverses
scene placement and clipping. Preview and inspector have independent, ordered
base/overlay buffers, preserving painter order and existing input behavior.
Initially hidden controls are retained and become visible/clickable on panning.
Scenes exceeding the existing 65,536-vertex retained-view range use the existing
clipped fixed-placement path. The regression suite covers that fallback and held
immutable snapshots.

Native input yields after at most 32 messages or after an event exhausts its
4 ms work budget. This is a cooperative budget, not a preemptive time guarantee;
no pointer edges or samples are discarded. State sync and renderer credit receive
a turn during continuous input.

Property intent submission retains local identity/type/batch checks without
compiling an unused optimistic draft. The authority still validates observations,
compiles valid activations and journals receipts atomically. Receiving a property
delta retains immutable-seed and active-closure validation, without repeating UI
package admission. Mount admission and recovery still perform full binding checks.

## Measurements on Frame

The [before](data/frame-oib-input-2026-11-02/before.json) and
[after](data/frame-oib-input-2026-11-02/after.json) records contain raw CPU samples
and observed native presentation acknowledgements. The baseline is release
`718ee4957c62c4a225bd0a6e05d3db9ddf28a2e239b87932c55c4bf1341b44ef`.

The CPU probe is the release ARMv7 build of
[`oib_input_cost`](../../crates/otrios-core/examples/oib_input_cost.rs), using the
real app, font, state adapter and composer at 1280×800. It times 100 idle ticks
and 24 samples per other operation after admission/initial composition. It uses
fresh in-memory default data and no network, durable journal or GPU. Median CPU
costs in milliseconds:

| Operation | Before | After |
| --- | ---: | ---: |
| OIB diagnostics | 34.946 | 0.020 |
| Idle tick | 0.471 | 0.058 |
| Slider composition | 38.990 | 5.708 |
| Panel composition | 7.394 | 2.938 |

The [input probe](data/frame-oib-input-2026-11-02/input-probe.py) offers one
three-second 30 Hz gesture through the existing calibrated evdev reader. It polls
native readiness after presentation at 5 ms intervals. It is synthetic device
software acceptance, not a human touch or photon-latency measurement. The live
fixture uses the existing wide Live UI, a 520 px component width and 56 px
inherited spacing. The hardware, rotation, calibration, governor and service
limits were not changed.

| Gesture | Baseline | Final release |
| --- | --- | --- |
| Inspector pan | 11.0 observed frames/s | 28.0 observed frames/s |
| Slider drag | One new presentation during the three-second capture; then stalled | 24.7 observed frames/s throughout the gesture |

Rates count only readiness publications while capture is active. The raw baseline
slider record also contains its initial pre-gesture readiness sample; that sample
is excluded from the summary above. These short runs establish improvement for
this UI; they do not establish 60 fps or a general hardware capacity limit.

Three native component-accent taps confirmed and presented in
750, 752, 749 ms, including an 80 ms held contact,
authority publication, validation and a 10 ms polling interval. Shared property
activation therefore still has a visible validation delay, separate from local
scroll/drag feedback. The cycle restored the previous accent, retained PID/mount,
and finished at active/presented generation 28 with released input.
[Button evidence](data/frame-oib-input-2026-11-02/buttons.json) and
[final readiness](data/frame-oib-input-2026-11-02/native-ready.json) verify the
running native executable against the signed manifest and healthy services.

## Validation and publication

Focused commands passed using Rust 1.90.0, locked/offline dependencies:

- Core `canvas` integration suite: 69 tests; `oib`: 9 tests.
- Core `dashboard::retained_text`: 5 tests, including oversized-view fallback.
- Dashboard `native_owner` integration target: 18 tests with local socket access.
  An earlier sandboxed attempt could not bind its sockets; the permitted rerun passed.
- Strict all-target Clippy for `otrios-core` and `otrios-dashboard` with
  `canvas,host-sync` features.
- Node `test_gpu_scene.mjs` and `test-canvas-worker.mjs`: 29 tests.
- Browser Wasm and ARM direct host/authority builds; complete signed closure
  verification, atomic activation and focused native acceptance.

The final frozen build inputs match the checkout. No dependencies, unsafe code,
services, identities, privileges, state wire fields or app schema migrations were
added. A compatible verified v21 code recovery kit remains available; a separate
rollback exercise was not repeated. This follow-up did not rerun `scripts/check.sh`,
a broad browser/Safari campaign or Linux shared-memory IPC acceptance. The new
rendering paths are shared by browser and native hosts; live performance samples
in this record are specifically from Frame.
