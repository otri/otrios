# OTriOS

OTriOS explores an operating environment built on Linux hardware support, immutable shared objects, reactive views, and streamable executable modules. This repository starts with a small Linux userspace proof of concept for the object model.

The repository contains experimental Rust contract types, immutable aggregate manifest validation, and a [shared dashboard experiment](docs/DASHBOARD.md): server-owned Merkle state with RAM-only client intents, immutable FlexBuffers view snapshots, and a Rust compositor shared by native OpenGL ES 2 and browser WebGL hosts. Touch updates stream through a persistent central research-room service. A private Tailscale HTTPS deployment supports signed, state-preserving code reloads on both hosts; see [live development](docs/DASHBOARD_LIVE.md). The photo frame target uses native graphics, without a browser. Its [primitive scene tree and responder chain](docs/SCENE_TREE.md) are shared by both hosts. [Binary layout templates and instances](docs/BINARY_LAYOUT.md) can be edited and streamed live without rebuilding their code. A [Prometheus monitoring view](docs/MONITORING.md) recreates the main service overview with seven days of actual history and five-minute refreshes. The [Home Screen and core keyboard](docs/HOME_AND_INPUT.md) provide local demo navigation, consistent Back/Home, and a shared primitive-based input lab.

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
| `web/dashboard` | Browser host for the shared compositor | Apache-2.0 |
| `interfaces` | IDL requirements before choosing a generator | Apache-2.0 |
| `docs` | Architecture, PoC milestones, and collaboration setup | Apache-2.0 |

Start with [the PoC plan](docs/POC.md), [architecture](docs/ARCHITECTURE.md), [Tess handoff](docs/TESS_HANDOFF.md), and [Cloud setup](docs/CODEX_CLOUD.md). See [contribution instructions](CONTRIBUTING.md).

## Licensing

The runtime uses LGPLv3 only; interfaces, SDK, documentation, and tooling use Apache 2.0. These are path-specific licenses, not a choice of either license for all files. Separate proprietary hardware, firmware, and accelerator implementations are an intended integration model, subject to the applicable licenses. See [the license boundary](LICENSE.md).

Live Controls now includes a [primitive component playground](docs/COMPONENT_PLAYGROUND.md): captured slider/dial/pad gestures, independent shared values and local feedback. The earlier editable dashboard remains under Layout Lab.

Animation and 3D experiments: [presentation ownership, load controls and hardware measurements](docs/ANIMATION.md). The [direct KMS display path](docs/DIRECT_DISPLAY.md) bypasses Weston, with [measured renderer improvements](docs/experiments/direct-display-validation.md) and an accepted [SDL-free Linux backend design](docs/rfcs/0001-direct-linux-gles2.md).

A [public module host with private Monitoring](docs/PUBLIC_ACCESS.md) serves a verified Wasm/UI replica from OrbStack. Public demo controls share the photo frame's state; Monitoring uses a separate authorized subscription. Code becomes active only after the configured replica is ready.

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
