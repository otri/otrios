<!-- SPDX-License-Identifier: Apache-2.0 -->
# Frame burst capacity qualification — 2026-10-20

## Corrected scope and targets

The earlier 4,096-socket and 512-mounted-reader results do not qualify complete
Component Playground clients. That app automatically enables the ink channel,
whose independent 16-subscriber cap was omitted by the previous generator.
The old measured data remain valid for their stated state-only workloads.

The primary target is 64 public protocol clients, each mounting the real signed
Component Playground and receiving an ink welcome on its shared v1 WebSocket.
128 is a stretch target. 256 is exploratory and only runs if the smaller cohorts
meet the gates. Production stays at 64 public / 4 trusted sessions during testing.
Ink admission must accommodate the configured combined peer quota, rather than
introduce a hidden lower subscriber cap. History, point-rate and message bounds
remain unchanged. This does not introduce a new state or control wire format.

Each sparse trial runs for 100 seconds with a seeded one-second gesture from
every client: 1% intended input duty per client at 30 Hz. Uniform, reproducible
start offsets allow overlap; intended duty, actual move counts and sampled
concurrency are recorded. Separate control-only, ink-only and mixed trials distinguish the paths.
Mixed clients alternate Linear, Rotary and Touch Pad gestures. Pointer capture,
normalized input, transient drawing and release use the actual core Client API.
All clients remain mounted and ink-subscribed while idle.

A separate aligned-burst probe starts 3 writers among 64 clients, or 4 among 128,
together. Those are the approximate point-in-time 99th-percentile writer counts
for independent 1% activity, not maximums or predictions for coordinated users.
A 256-client exploratory probe uses 7 simultaneous writers.

Prospective acceptance gates, fixed before collecting the new results:

- Every requested client mounts, receives its ink welcome and remains connected.
- At least 95% of scheduled move samples are offered; missed samples are counted.
  Drawing must also emit at least 95% of scheduled ink moves, with down/up
  boundaries verified, so an empty or heavily coalesced stream cannot pass.
- Ink delivery to other clients: p95 <=250 ms and p99 <=500 ms, measured from
  generator send to receipt, including the frame and Tailnet relay round trip.
- No missing or duplicate measured ink deliveries after draining; no frames are
  silently counted as delivered merely because the sender queued them.
- Gesture release to the writer's durable confirmation: p95 <=1 second.
- All clients reach equal confirmed four-field state within 2 seconds of the
  last gesture release. Ink history expiry is separate from state convergence.
- No client errors, production-health guard failure, OOM or target swap use.
  The existing 256 MiB target cap and 256 MiB global-headroom abort remain.

The generator runs outside the frame. These are protocol/client-core acceptance
criteria, not browser GPU/FPS, physical touch or Cloudflare Tunnel latency claims.
Test authorities use fresh disposable state while production services continue.
Raw telemetry remains local; only sanitized aggregate results are published.

## Downward search after the first miss

The 64-client aligned mixed probe missed the prospective ink and confirmation
latency gates while retaining every point and every connection. The remaining
64-client sparse profiles characterize the individual paths; 128 and 256 will
not be qualified by extrapolating from the connection result.

The downward search probes 32, then 16, then 8 clients as necessary. It keeps the
same 3-way aligned Linear/Rotary/Touch Pad collision across these cohorts using
`--burst-overlap 3`, so reduced fanout is the changed variable. This is stricter
than their independent-activity 99th-percentile count of 1–2 writers. A passing
aligned mixed/ink cohort must then pass the same three 100-second sparse
profiles, with the original latency and delivery gates unchanged. This tests
which smaller cohort meets the chosen service target; it does not redefine a
failed higher cohort as a pass or claim a maximum hardware connection count.

## Endpoint validation refinement

The initial 64-client profiles used a common final pad coordinate. After the
first stroke, later pad releases could confirm an unchanged value, making the
release-confirmation figures artificially cheap. Those runs remain diagnostic
connection/ink-delivery evidence, and 64 already missed the latency gates even
with that lighter release workload; they do not qualify a client cohort.

Final qualification uses per-client varied gesture endpoints and verifies that
each released gesture changes its selected fields from the state at gesture
start. An unchanged release fails the client gates. The same 30 Hz/duty,
ink-delivery, confirmation and convergence thresholds remain in force. New runs
use a distinct generator hash and output directories, retaining the original
fixed-endpoint measurements locally without overwriting them.

The interrupted fixed-endpoint diagnostic scheduler left one disposable test
service active; its exact authority, relay and sampler were inspected and removed
before final qualification began. The runner now handles SIGTERM through its
cleanup path, terminates its client process on interruption, and gives each test
authority a 360-second systemd lifetime bound. This changes test lifecycle only;
it does not alter the offered workload or latency gates.

## Varied-endpoint collision measurements

The same three simultaneous one-second gestures are used at each population.
All emitted points reached every other subscriber, all gestures changed their
selected fields, and the cases below had no disconnects, swap use or production
health failures. Passing this collision gate alone does not qualify the full
sparse-workload target. Latency is measured through the temporary Tailnet TCP
relay and includes that relay, network and generator cost.

| Clients | Collision | Ink p95 ms | Ink p99 ms | Release p95 ms | Final settle ms | Peak authority RSS MiB | Client gates |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | --- |
| 64 | aligned-mixed | 1245 | 1380 | 1391 | 1532 | 9.4 | Miss |
| 32 | aligned-mixed | 430 | 496 | 495 | 496 | 7.6 | Miss |
| 32 | aligned-ink | 512 | 607 | 557 | 941 | 7.6 | Miss |
| 16 | aligned-mixed | 168 | 240 | 219 | 228 | 6.5 | Pass |
| 16 | aligned-ink | 157 | 180 | 186 | 203 | 6.5 | Pass |

The 64-client mixed case missed both ink and confirmation gates. The 32-client
cases missed the ink gate; their state confirmation met the one-second target.
The 16-client collisions met every gate and proceed to sparse qualification.
These are measured service-target results, not RAM exhaustion or a hardware
maximum. The separate 64-connection production admission quota is unchanged.
