# OTriOS

OTriOS explores an operating environment built on Linux hardware support, immutable shared objects, reactive views, and streamable executable modules. This repository starts with a small Linux userspace proof of concept for the object model.

The scaffold currently contains experimental Rust contract types and immutable aggregate manifest validation with unit tests. Object storage, hashing, IPC, shared memory, subscriptions, module execution, and capability enforcement are not implemented yet.

## Build and check

Install Rust through [official rustup](https://rustup.rs/), then run:

```sh
bash scripts/cloud-install.sh
```

After setup, the current dependency-free workspace can be checked offline:

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
| `interfaces` | IDL requirements before choosing a generator | Apache-2.0 |
| `docs` | Architecture, PoC milestones, and collaboration setup | Apache-2.0 |

Start with [the PoC plan](docs/POC.md), [architecture](docs/ARCHITECTURE.md), [Tess handoff](docs/TESS_HANDOFF.md), and [Cloud setup](docs/CODEX_CLOUD.md). See [contribution instructions](CONTRIBUTING.md).

## Licensing

The runtime uses LGPLv3 only; interfaces, SDK, documentation, and tooling use Apache 2.0. These are path-specific licenses, not a choice of either license for all files. Separate proprietary hardware, firmware, and accelerator implementations are an intended integration model, subject to the applicable licenses. See [the license boundary](LICENSE.md).
