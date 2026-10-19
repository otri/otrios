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
start offsets allow overlap; actual move counts, duty and concurrency are
recorded. Separate control-only, ink-only and mixed trials distinguish the paths.
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
