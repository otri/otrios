# OTriOS

OTriOS explores an operating environment built on Linux hardware support, immutable shared objects, reactive views, and streamable executable modules. This repository starts with a small Linux userspace proof of concept for the object model.

The repository contains experimental Rust contract types, immutable aggregate manifest validation, and a [shared dashboard experiment](docs/DASHBOARD.md): server-owned Merkle state with RAM-only client intents, immutable FlexBuffers view snapshots, and a Rust compositor shared by native OpenGL ES 2 and browser WebGL hosts. Touch updates stream through a persistent central research-room service. A frame-hosted authority supports signed, state-preserving code reloads on both hosts; see [live development](docs/DASHBOARD_LIVE.md) and [frame hosting](docs/FRAME_HOSTING.md). The photo frame target uses native graphics, without a browser. Its [primitive scene tree and responder chain](docs/SCENE_TREE.md) are shared by both hosts. [Binary layout templates and instances](docs/BINARY_LAYOUT.md) can be edited and streamed live without rebuilding their code. A [Prometheus monitoring view](docs/MONITORING.md) recreates the main service overview with seven days of actual history and five-minute refreshes. The [Home Screen and core keyboard](docs/HOME_AND_INPUT.md) provide local demo navigation, consistent Back/Home, and a shared primitive-based input lab.

The general content store, Linux shared-memory IPC, arbitrary Canvas subtree subscriptions, OS module loader, and capability enforcement remain unimplemented. The Canvas app runtime executes bounded Wasm behavior; it does not complete the planned Linux runtime.

## Build and check

Install Rust through [official rustup](https://rustup.rs/), then run:

```sh
bash scripts/cloud-install.sh
```

After setup, the workspace (after dependencies have been fetched) can be checked offline:

```sh
bash scripts/check.sh
```

Rust 1.90.0 is pinned for reproducibility. Linux CI is configured for Ubuntu 24.04. macOS can run the scaffold checks; Linux is required for the planned shared-memory integration tests.

## Repository map

| Path | Purpose | License |
| --- | --- | --- |
| `crates/otrios-core` | Covered runtime implementation | LGPL-3.0-only |
| `crates/otrios-interfaces` | Experimental shared contract types | Apache-2.0 |
| `crates/otrios-sdk` | Client-facing SDK, currently contract reexports | Apache-2.0 |
| `crates/otrios-dashboard` | Thin native/browser hosts and demo sync server | Apache-2.0 |
| `web/canvas` | Browser host for the shared compositor | Apache-2.0 |
| `interfaces` | IDL requirements before choosing a generator | Apache-2.0 |
| `docs` | Architecture, RFCs, experiment evidence, and development guides | Apache-2.0 |

Start with [the PoC plan](docs/POC.md), [architecture](docs/ARCHITECTURE.md), and [development instructions](AGENTS.md). Follow the [contribution instructions](CONTRIBUTING.md) for scoped changes and review.

## Licensing

The runtime uses LGPLv3 only; interfaces, SDK, documentation, and tooling use Apache 2.0. These are path-specific licenses, not a choice of either license for all files. Separate proprietary hardware, firmware, and accelerator implementations are an intended integration model, subject to the applicable licenses. See [the license boundary](LICENSE.md).

Live Controls now includes a [primitive component playground](docs/COMPONENT_PLAYGROUND.md): captured slider/dial/pad gestures, independent shared values and local feedback. The earlier editable dashboard remains under Layout Lab.

Animation and 3D experiments: [presentation ownership, load controls and hardware measurements](docs/ANIMATION.md). The [direct KMS display path](docs/DIRECT_DISPLAY.md) bypasses Weston, with [measured renderer improvements](docs/experiments/direct-display-validation.md) and an accepted [SDL-free Linux backend design](docs/rfcs/0001-direct-linux-gles2.md).

[Retained text viewports and culling](docs/rfcs/0017-retained-text-viewports.md)
and [adaptive alpha coverage storage](docs/experiments/native-retained-simd-2026-10-27.md)
reach 56.4–56.6 kernel-presented fps for dense scrolling text on the Mali-400 frame.
The full Layout Lab demo measures about 42 fps; these averages do not imply a
60 fps lock. Its **Text scroll demo** opens a full-page, two-axis momentum workload
at `/layout-lab/text-scroll`, with retained geometry and viewport culling.

The [fixed point/SIMD evaluation](docs/rfcs/0018-fixed-point-layout-simd.md)
found a faster isolated integer SIMD kernel, but no complete scrolling gain over
the optimized float path and slower rebuilds. The pilot and unsuccessful GPU
position formats are retired. [RFC 0019](docs/rfcs/0019-native-fixed-simd-pipeline.md)
records the outcome and evidence required for any future float SIMD batches.

A [public module host with private Monitoring](docs/PUBLIC_ACCESS.md) serves a verified Wasm/UI replica through the single Rust authority. Public demo controls share the photo frame's state; Monitoring uses a separate authorized subscription. Code becomes active only after the configured replica is ready.

The [Canvas app packages](apps/README.md) are the dashboard runtime on both hosts. Home mounts Component Playground, Layout Lab, Input Lab, Animation Lab, 3D Lab and private Monitoring through independently admitted UI/State references. Each app owns its source dependencies and scoped state; animation clocks, editing focus and ephemeral trails stay in RAM. See [RFC 0004](docs/rfcs/0004-canvas-behavior-and-app-packages.md) for scope and execution limits.

The [modern app pattern and retirement map](docs/MODERN_APP_PATTERN.md) defines the target ownership model, current implementation limits and concrete gates for deleting the earlier dashboard paths.

The [bounded Merkle hierarchy and subtree transport](docs/rfcs/0005-merkle-subtree-prototype.md)
now provide immutable incremental hashing, causal multi-value registers, scoped
replication/resync and partial-subtree joins. The live [Canvas state adapter](docs/rfcs/0006-durable-canvas-merkle-adapter.md) uses app-scoped Merkle replication,
server-side field policy and durable intent receipts; pending edits stay in RAM.
Browser/frame [deployment acceptance](docs/experiments/canvas-merkle-adapter-validation.md) records the cutover. The pre-Merkle baseline is tagged
`2026-10-19-canvas-apps-milestone`; performance results distinguish actual tree
updates from network, durable storage and display latency.

The [photo-frame Canvas adapter comparison](docs/experiments/canvas-adapter-frame-benchmark.md) measures durable latency, native presentation, application/device CPU and actual replication traffic against the dated pre-Merkle milestone. It also isolates a binary-envelope encoding cost; measured results distinguish application CPU savings from total system cost.

The [practical Canvas foundation](docs/rfcs/0010-runtime-foundation.md) now deploys
one bounded RAM intent queue and coherent UI-derived Merkle coverage, including
only the writer's own receipt. The global dashboard replica and Automerge dependency
are retired; [resource diagnostics](docs/ARCHITECTURE-RESOURCES.md) are shared by both hosts.
[Browser/frame qualification and measurements](docs/experiments/canvas-foundation.md)
record lower replication traffic and mixed latency results. The final
[retirement and browser/frame acceptance](docs/experiments/runtime-retirement.md)
closes this bounded foundation milestone, including explicit validation costs.

The [bounded replication increments](docs/rfcs/0007-bounded-replication-pipeline.md)
fix opaque-byte expansion, make signal history opt-in and reuse bounded encoding
workspace, with a separately measured interactive-socket change. Their
[frame measurements](docs/experiments/canvas-replication-pipeline.md) compare each
increment against its immediate control using fresh disposable authorities.
These increments are [deployed on browser and frame](docs/experiments/release-discovery-deployment.md). Release discovery uses a stable [hello/upgrade channel](docs/rfcs/0008-release-control.md) inside the [unified server session](docs/rfcs/0009-unified-server-session.md), independently of state decoding ([deployment acceptance](docs/experiments/release-hello-deployment.md)). [Sync v4 packed envelopes and measured optional compression](docs/rfcs/0011-packed-canvas-transport.md) are deployed. Normal state traffic stays raw after frame measurements; the optional bulk compressor is bounded.

[Public UI reuse and optional iPad navigation timings](docs/rfcs/0016-ui-cache-and-navigation-timings.md) extend the packed transport with sync v5. Repeat mounts reuse admitted public UI by hash while requesting fresh State. Open `/?timings=1` for local startup and navigation phase reports.
