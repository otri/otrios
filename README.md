# OTriOS

OTriOS explores an operating environment built on Linux hardware support, immutable shared objects, reactive views, and streamable executable modules. This repository starts with a small Linux userspace proof of concept for the object model.

The repository contains experimental Rust contract types, immutable aggregate manifest validation, and a [shared dashboard experiment](docs/DASHBOARD.md): server-owned Merkle state with RAM-only client intents, immutable FlexBuffers view snapshots, and a Rust compositor shared by native OpenGL ES 2 and browser WebGL hosts. Touch updates stream through a persistent central research-room service. A frame-hosted authority supports signed, state-preserving code reloads on both hosts; see [live development](docs/DASHBOARD_LIVE.md) and [frame hosting](docs/FRAME_HOSTING.md). The photo frame target uses native graphics, without a browser. Its [primitive scene tree and responder chain](docs/SCENE_TREE.md) are shared by both hosts. [Binary layout templates and instances](docs/BINARY_LAYOUT.md) can be edited and streamed live without rebuilding their code. A [Prometheus monitoring view](docs/MONITORING.md) recreates the main service overview with seven days of actual history and five-minute refreshes. The [Home Screen and core keyboard](docs/HOME_AND_INPUT.md) provide local demo navigation, clickable ancestor breadcrumbs, and a shared primitive-based input lab.

The general content store, Linux shared-memory IPC, arbitrary Canvas subtree subscriptions, OS module loader, and capability enforcement remain unimplemented. The Canvas app runtime executes bounded Wasm behavior; it does not complete the planned Linux runtime.

The [app packages](apps/README.md) include The Brass Relay, Test Playground and private Monitoring. The single negotiated session carries release control, scoped state and ephemeral ink; see [live deployment](docs/DASHBOARD_LIVE.md) and [public access](docs/PUBLIC_ACCESS.md). The [RFC index](docs/rfcs/README.md) and [milestone history](docs/HISTORY.md) link designs and qualification evidence. The [modern app pattern](docs/MODERN_APP_PATTERN.md) records the current ownership model and limits.

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
