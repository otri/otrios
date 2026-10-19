<!-- SPDX-License-Identifier: Apache-2.0 -->
# Frame authority cutover and load test — 2026-10-20

The later [complete-client burst qualification](frame-burst-capacity-2026-10-20.md)
corrects the interactive capacity scope: these original state-only clients did
not enable the Playground ink subscription. Their connection/memory results do
not qualify complete drawing clients or remote drawing latency.

The 1 GB RK3126 frame now owns the OTriOS authority and the public site's
Cloudflare Tunnel. Monitoring is collected directly from the existing Linode
Prometheus HTTPS API over Tailscale. Grafana and Prometheus remain on Linode.
The Mac builds and signs releases; it is no longer the serving or metrics
collection dependency. [Frame operations](../FRAME_HOSTING.md) describes the
service layout and deployment protocol.

## Cutover evidence

The stopped authority state and private presentation were copied to the frame
with matching SHA-256 digests for 22 files. A quiet cold-recovery check preserved
all 21 canonical app checkpoint/journal files across seven roots. The initial
live comparison included a legitimate connected-client journal append and was
not used as evidence of unchanged files. Release control remains v1 on `/session`;
the Canvas state codec remains v4. No canonical state format changed.

The public conformance checker passed 68 unauthenticated checks on
`https://otrios.otri.ca`. Its complete 81-check mode passed against a disposable
ARM authority with a known disposable login hash. The production login hash was
preserved. The native renderer reported signed release
`9b27ab25e005a16d12eb3631b0885c830bc8e230b7b2a2f214f7a0d439ef1312`, confirmed
state, GLES2 direct rendering, calibrated touch and RGB565 scanout, with its
code and shared WebSocket using the frame's trusted loopback listener. A Mac
publisher run successfully activated and served a complete signed release on
the frame. Direct Prometheus collection completed in about nine seconds and
published a 16,568-byte presentation; the timer repeats every five minutes.

Obsolete immutable code replicas were removed from the Mac staging store and
frame cache, reclaiming approximately 1.2 GB. Each store retains its active
release. Durable app state and unrelated hardware recovery backups were retained.
Mac authority, tunnel and metrics collector LaunchAgents were disabled.

There was no real browser GUI available in this session. Public HTTP/WebSocket
conformance, Node client/worker tests and native readiness are the browser/native
evidence; they do not establish physical touch, color or remote browser rendering
performance. No kernel, graphics, calibration, CMA or zram configuration changed.

## Method

The external Rust generator uses the actual core Canvas client, app catalog and
Component Playground behavior. Every cycle changes Linear, Rotary and both Touch
Pad coordinates through the app's normalized event handlers. This is a state-write
workload: normalized Touch Pad assignments repeatedly
change durable coordinates; physical Touch Pad motion normally uses transient
preview until release. It therefore does not reproduce pointer hit testing or
claim the same write rate as an ordinary finger gesture. The server runs
fresh disposable state, using the production immutable code replica. Production
state is never mounted by the load test. The production authority, display,
collector and tunnel remain running throughout the measured ramps.

The transport is a temporary Tailscale-only TCP relay to the disposable public
listener. Client ACK timings include network, relay, generator and durable server
work. They are not browser input-to-pixel timings or Cloudflare capacity tests.
A 256 MiB hard cgroup limit and 192 MiB memory high apply to the test authority;
the relay has its own 256 MiB hard limit. A memory-headroom/production-health
guard aborts the run on available RAM below 256 MiB or a failed production probe.
The runner removes the relay, units and disposable state afterward.

One-second samples record process RSS, cgroup memory, CPU and production health.
CPU uses 100% for one core; the four-core system has a 400% service maximum.
Peak RSS covers setup and the measured interval. Steady CPU samples are selected
by the generator's input/settle timestamps, with approximately one-sample boundary
uncertainty. Table CPU averages use only the requested input period, excluding
settle time. Baseline samples predate added swap instrumentation: high-memory
baseline cases must not be described as swap-free. Cgroup accounting and RSS
measure different sets of pages; neither should be silently substituted for the
other.

The baseline server uses the WebSocket library's 128 KiB initial read buffer.
The candidate explicitly reserves 4 KiB, while retaining the existing maximum
message/frame sizes. Both binaries use the same pinned dependencies and ARMv7
release build. Baseline binary SHA-256:
`f96324a397450f13ad6c423000639f8a4a3e7d45a9510c015de0121a7f4c1e96`.
Candidate binary SHA-256:
`da471a65a81fe70af6eefe83a320f1649bc51fa9578d71cdf69fb0b51bec1f0f`.

The baseline ramp uses 15-second holds: 64, 256, 512, 1,024, 2,048 and 4,096
negotiated idle sockets; 32, 64, 128, 256 and 512 mounted readers; then 1, 2, 4,
8, 16, 32 and 64 writers among 64 mounted clients. In this confirmation-limited
mode each writer requests 10 cycles/second, waiting for its own durable ACK before
starting another cycle. An overloaded case therefore reduces achieved throughput;
its ACK latency cannot be interpreted as maintaining the requested offered rate.

## Baseline measurements

All mounted cases connected their requested clients and converged. The 4,096
socket attempt admitted 3,129 before five-second hello deadlines failed; it is
a failed capacity point, even though surviving sockets stayed connected.
Memory high reclaim was active in the large baseline cases, so this is a
bounded-service result rather than a physical RAM ceiling. No production
health guard fired.

| Workload | Connected / requested | Peak authority RSS MiB | Input CPU (% of one core) |
| --- | ---: | ---: | ---: |
| Negotiated idle sockets | 64 / 64 | 14.0 | 1.0 |
| Negotiated idle sockets | 256 / 256 | 40.0 | 5.2 |
| Negotiated idle sockets | 512 / 512 | 74.1 | 10.2 |
| Negotiated idle sockets | 1,024 / 1,024 | 143.1 | 24.5 |
| Negotiated idle sockets | 2,048 / 2,048 | 190.0 | 73.2 |
| Negotiated idle sockets | 3,129 / 4,096 | 190.6 | 120.6 |
| Mounted readers | 32 / 32 | 11.2 | 2.1 |
| Mounted readers | 64 / 64 | 17.3 | 1.0 |
| Mounted readers | 128 / 128 | 29.3 | 5.0 |
| Mounted readers | 256 / 256 | 53.1 | 10.2 |
| Mounted readers | 512 / 512 | 101.0 | 23.6 |

For 64 mounted clients, the requested rate is 10 complete control cycles per
second per writer. Achieved cycles/second is **aggregate**, not per client.

| Active writers | Achieved cycles/s | ACK p50 ms | ACK p95 ms | Input CPU (% of one core) | Settle after stop ms |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 3.7 | 270 | 412 | 71.2 | 677 |
| 2 | 4.8 | 437 | 681 | 73.6 | 1930 |
| 4 | 6.7 | 553 | 1329 | 55.7 | 692 |
| 8 | 12.2 | 687 | 1011 | 67.1 | 1269 |
| 16 | 20.6 | 784 | 1165 | 61.5 | 888 |
| 32 | 27.4 | 1050 | 2707 | 47.1 | 1287 |
| 64 | 44.3 | 1426 | 2517 | 43.2 | 1898 |

All baseline writer cases reached equal confirmed four-field state after input
stopped. Peak authority RSS stayed between 17.4 and 18.0 MiB. At 64 writers,
the achieved rate was 44.3 cycles/second in aggregate, far below the requested
640; holding 64 sockets does not establish 640 durable cycles/second.

Raw telemetry stays locally in the ignored `target/frame-stress-*` directories.
This report publishes aggregate synthetic benchmark results only; connection
addresses, private paths, process identifiers and raw service samples are omitted.

## Smaller-buffer idle comparison

The same baseline client and 15-second idle hold were used against the 4 KiB
server candidate. All requested sessions connected, with no client errors,
production-health failures, memory-high events or swap use. This is a successful
4,096-session test under the stated limits, not a physical maximum or a claim
about 4,096 browsers through Cloudflare.

| Idle sockets | Baseline peak RSS MiB | Candidate peak RSS MiB | Candidate input CPU (% of one core) |
| ---: | ---: | ---: | ---: |
| 64 | 14.0 | 5.9 | 1.0 |
| 512 | 74.1 | 10.2 | 7.7 |
| 2,048 | 190.0 | 25.0 | 39.5 |
| 4,096 | 190.6 (partial admission) | 44.1 | 63.4 |

At 512 idle sessions, peak process RSS fell from 74.1 to 10.2 MiB, approximately
an 86% reduction. At 4,096 sessions the candidate used 44.1 MiB peak RSS and
57.2 MiB peak cgroup memory, with at least 734 MiB system memory available.
Connection setup took 90 seconds at the largest count; the final hold
lasted 15 seconds. These short single trials are not a soak test, and CPU/latency
differences should not be treated as statistically controlled speedups.

## Invalid setup probes

An initial SSH-forwarding probe could not establish the test transport because
forwarding was disabled; no capacity result was taken from it. The measured runs
use the scoped Tailscale relay instead. A later continuous-input launch failed
because a copied generator lacked execute permission; no input load was generated.
The copy was corrected and that cohort was rerun into a fresh output directory.
The runner now rejects nonexecutable clients before remote setup and tears down
its sampler on launch exceptions. A deliberate invalid-executable-format probe
verified sampler, authority, relay and temporary-file cleanup in 32 seconds.
Neither setup failure is counted as an authority overload or benchmark result.

## Candidate subscribed-client and writer results

The 512 mounted-reader rerun connected every client and converged, using
36.7 MiB peak authority RSS versus 101.0 MiB in the baseline.
It used no swap. The confirmation-limited writer rerun used the same 64-client
cohort, 10 Hz request and 15-second input interval as the baseline:

| Active writers | Achieved aggregate cycles/s | ACK p50 ms | ACK p95 ms | Peak RSS MiB | Input CPU (% of one core) |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 4.4 | 229 | 330 | 9.6 | 64.4 |
| 2 | 5.4 | 386 | 631 | 9.8 | 63.3 |
| 4 | 8.5 | 466 | 832 | 9.7 | 62.2 |
| 8 | 14.1 | 542 | 849 | 9.8 | 61.4 |
| 16 | 22.3 | 711 | 1084 | 10.0 | 55.3 |
| 32 | 33.5 | 888 | 1726 | 9.4 | 46.0 |
| 64 | 45.6 | 1387 | 2484 | 9.8 | 38.2 |

## Continuous 30 Hz input

These cases use the new generator mode: input continues on its clock while
confirmation is outstanding, and a final release flushes the remaining intents.
Each cycle offers three normalized control events (four coordinates). The real
client queue can coalesce pending values. Offered cycles are not durable commits;
fully confirmed spans are not per-event ACK samples. We therefore report offered
rate and final convergence delay rather than presenting span timings as input
latency. Every row uses a ten-second input interval.

| Mounted clients | Active writers | Requested aggregate cycles/s | Offered aggregate cycles/s | Peak RSS MiB | Input CPU (% of one core) | Settle after stop ms | Converged |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 64 | 1 | 30 | 29.1 | 10.0 | 67.5 | 519 | Yes |
| 64 | 2 | 60 | 58.0 | 9.6 | 51.9 | 991 | Yes |
| 64 | 4 | 120 | 115.8 | 9.6 | 53.1 | 2420 | Yes |
| 64 | 8 | 240 | 232.2 | 9.8 | 57.9 | 1335 | Yes |
| 64 | 16 | 480 | 463.9 | 9.6 | 52.0 | 1336 | Yes |
| 64 | 32 | 960 | 919.0 | 9.8 | 43.2 | 1813 | Yes |
| 64 | 64 | 1920 | 1832.4 | 9.7 | 36.2 | 3144 | Yes |
| 256 | 32 | 960 | 902.6 | 21.9 | 60.5 | 9291 | Yes |

The generator records its own loop durations and actual cycle count, so a late
input loop cannot silently be counted as maintaining 30 Hz. Raw per-case samples
and diagnostics remain in ignored local target directories. These trials include
network/relay cost and background production activity; they do not isolate a
single causal source of confirmation delay. Authority CPU averages stay below
one core in the high-writer cases; these data do not establish a CPU-only
bottleneck. Durable I/O, replication/protocol waits, relay and network scheduling
need separate profiling. No claim is made about browser FPS,
physical frame input latency, a maximum sustainable active-user count or long-term
thermal/soak behavior.

The connection result supports keeping a 64-public-session production limit with
ample memory headroom. Shared-state churn remains substantially more expensive
than idle sockets. The buffer change addresses the idle memory reservation; it
does not make every intermediate control value a durable low-latency update.

## Commands and validation scope

The local checks used the configured Rust 1.90.0 environment:

```sh
cargo fmt --all -- --check
bash scripts/check.sh
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
node --test scripts/test-dashboard-session.mjs scripts/test-dashboard-monitoring-session.mjs \
  scripts/test-dashboard-artifact-cache.mjs scripts/test_gpu_scene.mjs \
  scripts/test-app-catalog.mjs scripts/test-canvas-worker.mjs
```

All passed, including real server process/durability tests, 28 release/supervisor
checks, three frame-hosting checks and 40 Node tests. The Linux GitHub CI run for
the tested code also passed. ARMv7 release builds passed for the authority,
Monitoring renderer, native display and bootstrap helper. No crate dependencies,
unsafe code, state codec or canonical-state formats were added or changed.
These checks do not complete the separate shared-memory Linux PoC gates.

The stress generator is built with:

```sh
cargo build --release --locked -p otrios-dashboard \
  --features canvas,host-sync --bin canvas-session-stress
```

For each binary pair, the runner command uses private configuration and catalog
paths, and a fresh output directory. The baseline uses the default full ramp;
the candidate uses `--socket-counts 64,512,2048,4096 --mounted-counts 512` with
otherwise identical 15-second settings. Continuous cases use
`--phase active --open-loop --clients 64 --seconds 10 --hz 30`; the final fanout
case adds `--clients 256 --active-counts 32 --max-active 32`. Hardware cases run
sequentially, with isolated state and a new authority for each count. The
same preserved baseline client binary is used for the buffer comparison;
continuous input uses the separately recorded updated generator binary.

## Final deployed acceptance

The deployed authority's SHA-256 matches the measured 4 KiB candidate exactly.
The signed browser/native release is unchanged from the cutover release above.
The final publisher input fingerprint matches the committed source inputs; later
report-only commits do not change executable inputs.

The running unit admits four trusted and 64 public sessions. The live admission
probe reached both limits, received 503 for the additional connections, and
observed confirmed native readiness at capacity. The final public-origin checker
passed all 68 anonymous checks; the newly deployed ARM binary also passed the
full 81-check disposable-login qualification. The native process has an
established loopback connection, matching signed release, confirmed state and no
pending commands. Direct Monitoring refreshed successfully after deployment.

The authority, tunnel, timer and native supervisor are enabled for boot; no
physical reboot was performed. Mac authority, tunnel and collector LaunchAgents
remain disabled. The build/signing watcher is running, and the sole retired Mac
Serve handler was removed. Each Mac staging, frame authority and native-cache
store contains exactly its active immutable release. Disposable test services,
relay files, state directories and provisioning staging were removed; temporary
credential copies on the build machine were deleted. Raw synthetic test telemetry
remains local and outside Git.
