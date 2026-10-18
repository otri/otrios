# OTriOS

OTriOS explores an operating environment built on Linux hardware support, immutable shared objects, reactive views, and streamable executable modules. This repository starts with a small Linux userspace proof of concept for the object model.

The repository contains experimental Rust contract types, immutable aggregate manifest validation, and a [shared dashboard experiment](docs/DASHBOARD.md): Automerge state, immutable FlexBuffers view snapshots, and a Rust compositor shared by native OpenGL ES 2 and browser WebGL hosts. Touch updates stream through a persistent central research-room service. A private Tailscale HTTPS deployment supports signed, state-preserving code reloads on both hosts; see [live development](docs/DASHBOARD_LIVE.md). The photo frame target uses native graphics, without a browser. Its [primitive scene tree and responder chain](docs/SCENE_TREE.md) are shared by both hosts. [Binary layout templates and instances](docs/BINARY_LAYOUT.md) can be edited and streamed live without rebuilding their code. A [Prometheus monitoring view](docs/MONITORING.md) recreates the main service overview with seven days of actual history and five-minute refreshes. The [Home Screen and core keyboard](docs/HOME_AND_INPUT.md) provide local demo navigation, consistent Back/Home, and a shared primitive-based input lab.

The general content store, Linux shared-memory IPC, subscriptions, module execution, and capability enforcement remain unimplemented. Dashboard view hashing and publication are a bounded in-process experiment, not completion of the planned Linux runtime.

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
