<!-- SPDX-License-Identifier: Apache-2.0 -->
# Frame burst capacity qualification — 2026-10-20

**Result:** 16 complete clients met the declared shared-room responsiveness gates
in this qualification. The 32- and 64-client collision probes missed the ink
latency budget. Production admission remains 64 public / 4 trusted sessions;
admission does not promise this drawing latency at full occupancy.

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

## Why these limits

The 64-public-session admission quota preserves the existing deployment target;
it is not a drawing-latency guarantee. Idle socket memory and active shared-room
fanout need separate qualification. Ink subscriptions now match all 4 trusted plus
64 public permits so that an admitted client can actually use the app.

Under independent 1% activity, the instantaneous active-writer count is
binomial with mean `0.01 × clients`. At 64 clients that mean is 0.64, and three
writers cover approximately 99% of observation instants. This is not a 99%
probability that an entire session avoids a larger collision. Coordinated users
can also exceed it. The aligned probe deliberately exercises that collision,
while the sparse profiles measure the proposed duty cycle.

Broadcast work grows faster than socket count: with every gesture drawing,
average move delivery demand is approximately `0.01 × clients × 30 × (clients−1)`.
That is 72 remote move deliveries/second at 16 clients, 298 at 32, and 1,210 at
64, before stroke boundaries, state replication or other overhead. These are
offered-work estimates, not measured server throughput. Thus 128 is a stretch
and 256 exploratory; neither follows from proving that thousands of idle sockets
fit in RAM.

The 250/500 ms ink thresholds bound typical and tail delay visible to another
viewer. They are provisional usability budgets, not a claim that local drawing
tolerates that delay; local previews are a separate path. The one-second durable
release budget allows slower persistence after a gesture, and the two-second
convergence gate bounds residual backlog. The 95% sample floor prevents a slow
generator from passing by offering much less work. Zero lost measured deliveries
checks correctness independently of latency. A 256 MiB authority cap and 256 MiB
remaining-system-memory guard reserve room for the native renderer, tunnel, OS
and transient load; they do not establish the host's physical capacity.

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
The 16-client collisions met every gate and passed sparse qualification below.
These are measured service-target results, not RAM exhaustion or a hardware
maximum. The separate 64-connection production admission quota is unchanged.

## Varied-endpoint sparse measurements

Each profile kept all 16 clients mounted and ink-subscribed for 100 seconds.
Every client completed one one-second gesture at 30 Hz, with all 480 scheduled
moves offered and all 16 releases changing their selected fields. The seeded
schedule sampled active writers every 100 ms: p50 0, p95 1, p99 2 and maximum 2
in each profile. The separate three-writer collisions test a larger overlap.

| Profile | Ink p95 ms | Ink p99 ms | Release p95 ms | Final settle ms | Remote point deliveries | Peak authority RSS MiB | Gates |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| Linear / Rotary | — | — | 281 | 273 | 0 | 6.4 | Pass |
| Touch Pad | 145 | 185 | 222 | 196 | 7,680 / 7,680 | 6.5 | Pass |
| Mixed | 141 | 185 | 224 | 195 | 2,400 / 2,400 | 6.5 | Pass |

Touch Pad emitted all 480 scheduled moves plus 16 down/up pairs; mixed emitted
all 150 drawing moves plus five down/up pairs. There were no missing, duplicate
or unknown measured deliveries, failed peers, client errors, production health
failures, OOM events or swap use. Available system memory stayed above 804 MiB
throughout these three profiles. Authority CPU averaged approximately 5–7% of
one core over the sampled measurement interval; this average includes idle gaps
and is not a burst CPU ceiling. Relay CPU averaged approximately 3–4% of one core
and peak relay RSS was 4.6 MiB, accounted separately from the authority. Generator
loop maxima were 2–3 ms, and no scheduled move samples were skipped.

The resulting target is **16 complete clients sharing this room**, under the
declared 1% intended duty and three-writer collision tests. It is not a claim
that 17 fails, a maximum idle connection count, or qualification of 16 sustained
writers. 128 and 256 were deferred after the lower cohorts missed their gates;
8 and 4 were unnecessary after 16 passed. These are short trials with one fixed
seed, not repeated-run confidence bounds or soak tests. Native rendering stayed
running but did not subscribe to the disposable test room; browser rendering,
physical drawing and public Cloudflare latency remain outside this acceptance.
The measured end-to-end delay does not isolate server, relay, network, persistence
or generator costs, so low average CPU/RSS alone cannot identify the bottleneck.

## Reproduction and provenance

Build `canvas-burst-stress` with `canvas,host-sync`, then use the private pinned
SSH configuration and ARM binary with the command in
[frame operations](../FRAME_HOSTING.md). Final collision probes used
`--phase burst --cohorts N --profiles idle,aligned-mixed,aligned-ink
--burst-overlap 3 --seconds 10 --hz 30` at 32 and 16 clients; 64 used only
`aligned-mixed`. The passing 16-client sparse probe used
`--profiles sparse-control,sparse-ink,sparse-mixed` with the same options; each
sparse profile always lasts 100 seconds. Every authority used fresh state.

Final generator SHA-256:
`8bc51b7c3e997d7f3086e5f16115cf0bae44f7f485042ba0b61d6db49b98e901`.
ARM authority SHA-256:
`915f02404b2b44b493b24ffe73f6ca17fdf3e5f553d7e2dd5377fd1447d56b07`.
The generator includes the changed-endpoint validation from commit `da371ab`;
the runner includes bounded lifetime/SIGTERM cleanup from `5cda01f`. Raw result,
metadata and one-second sample files remain in ignored local `target/frame-varied-*`
directories. No private addresses, credentials or raw telemetry are published.

## Final checks and live verification

After the search completed and removed its disposable services, a four-client
idle probe was interrupted with SIGTERM during sampling. The runner exited 143,
removed its authority, relay units and state, and production remained healthy.
This is a lifecycle check, excluded from the performance measurements.

With pinned Rust 1.90.0, `bash scripts/check.sh` and
`cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings`
passed. All 40 Node session, Monitoring, artifact-cache, GPU, catalog and worker
tests passed. The real-process ink-admission regression mounts 24 ink-enabled
public sessions, verifies broadcast delivery and checks permit recovery.

The publisher rebuilt the ARMv7 host artifacts and activated the same signed
renderer release. Live authority bytes matched the benchmarked server digest;
health reported `[4,64]` peer limits and ink capacity 68. The deployed binary
passed 81 host checks with a disposable login, and
`python3 scripts/test-dashboard-host.py --url https://otrios.otri.ca --public-only`
passed 68 production checks. Direct Prometheus refresh succeeded and native
readiness showed confirmed state with no pending commands. The build-only Mac
watcher was restored; the retired Mac authority, tunnel and collector remained
unloaded. Frame authority, native cache and Mac staging each retained only their
active signed release. No stress state or relay unit files remained.
