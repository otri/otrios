# OTriOS

OTriOS explores an operating environment built on Linux hardware support, immutable shared objects, reactive views, and streamable executable modules. This repository starts with a small Linux userspace proof of concept for the object model.

The repository contains experimental Rust contract types, immutable aggregate manifest validation, and a [shared dashboard experiment](docs/DASHBOARD.md): server-owned Merkle state with RAM-only client intents, immutable FlexBuffers view snapshots, and a Rust compositor shared by native OpenGL ES 2 and browser WebGL hosts. The photo frame uses native graphics; the browser uses WebGL. [Frame hosting](docs/FRAME_HOSTING.md) covers the single authority, signed updates, operation and rollback. Start with the [documentation index](docs/README.md) for app, rendering, input and Monitoring guides.

The general content store, Linux shared-memory IPC, arbitrary Canvas subtree subscriptions, OS module loader, and capability enforcement remain unimplemented. The Canvas app runtime executes bounded Wasm behavior; it does not complete the planned Linux runtime.

[Standalone Canvas app bundles](docs/rfcs/0025-standalone-app-bundles.md) add flat
FlexBuffers metadata, signed immutable packages and a registry installation path
that reuses qualified host binaries. [Bundle Counter](apps/bundle-counter/README.md)
builds outside the host source registry. Native Wasm AOT compilation remains a
[separate proposed experiment](docs/rfcs/0026-target-wasm-compilation.md).

The [app packages](apps/README.md) include The Brass Relay, Test Playground and private Monitoring. The single negotiated session carries release control, scoped state and ephemeral ink; see [live deployment](docs/FRAME_HOSTING.md) and [public access](docs/PUBLIC_ACCESS.md). The [RFC index](docs/rfcs/README.md) and [milestone history](docs/HISTORY.md) link designs and qualification evidence. The [modern app pattern](docs/MODERN_APP_PATTERN.md) records the current ownership model and limits.

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

For a read-only inventory of disposable local caches, run
`python3 scripts/clean-artifacts.py`. Add `--apply` to remove eligible caches;
see [maintenance](docs/MAINTENANCE.md) for scope and builder checks.

## Repository map

| Path | Purpose | License |
| --- | --- | --- |
| [apps/](apps/README.md) | App manifests, declarative UI, state schemas and optional Wasm reducers; includes Home and Monitoring | Apache-2.0 |
| [tools/canvas/](tools/canvas) | App authoring helpers and reusable UI components | Apache-2.0 |
| [crates/otrios-core/](crates/otrios-core) | Runtime implementation: Canvas, layout/compositor, Merkle state, collections and bundle validation | LGPL-3.0-only |
| [crates/otrios-interfaces/](crates/otrios-interfaces) | Shared contracts and wire types | Apache-2.0 |
| [crates/otrios-sdk/](crates/otrios-sdk) | Client-facing SDK, currently contract reexports | Apache-2.0 |
| [crates/otrios-dashboard/](crates/otrios-dashboard) | Authority server, native host, app packer and bundle CLI | Apache-2.0 |
| [crates/otrios-linux-display/](crates/otrios-linux-display) | Linux DRM/GBM/EGL display and device integration | Apache-2.0 |
| [crates/otrios-web-host/](crates/otrios-web-host) | Wasm bindings for the shared runtime | Apache-2.0 |
| [web/canvas/](web/canvas) and [web/shared/](web/shared) | Browser shell, worker, rendering adapters, session and artifact transport | Apache-2.0; third-party files retain their notices |
| [scripts/](scripts) | Checks, packaging, signed publication, app installation, deployment and probes | Apache-2.0 |
| [tools/dashboard/](tools/dashboard) | Frame service definitions and host provisioning files | Apache-2.0 |
| [interfaces/](interfaces) | IDL requirements before choosing a generator | Apache-2.0 |
| [docs/](docs/README.md) | Current guides, RFCs, milestone history and qualification evidence | Apache-2.0 |

Useful entry points are [canvas_bundle.rs](crates/otrios-dashboard/src/canvas_bundle.rs)
for package export/signing and Home composition,
[install-app.py](scripts/install-app.py) for installing/removing signed apps,
[watch-dashboard.py](scripts/watch-dashboard.py) for complete builds, and
[deploy-dashboard-frame.py](scripts/deploy-dashboard-frame.py) for Frame transfer.
Generated Cargo/build outputs live under ignored `target/`; they are not app source.

## Live system map

The **Frame** owns the authority and durable app state. It also runs the native
display and the public HTTPS tunnel. The **Mac** builds and signs releases; it is
not the live state server. The **browser** loads the shared Wasm runtime and keeps
its unconfirmed edits in RAM. See [Frame hosting](docs/FRAME_HOSTING.md) for operation.

| Location | Contents |
| --- | --- |
| Frame: `/opt/otrios-authority/` | Authority and Monitoring binaries, bundle checker and management helpers |
| Frame: `/opt/otrios-dashboard/` | Native supervisor and session bootstrap helper |
| Frame: `/etc/otrios-authority/` | Private service/auth/tunnel configuration and publisher public key |
| Frame: `/etc/otrios-dashboard.conf` | Native host endpoints; touch calibration remains in `/etc/udev/rules.d/99-spd-touch.rules` |
| Frame: `/var/lib/otrios-authority/releases/current.json` | Pointer to the complete active signed release |
| Frame: `/var/lib/otrios-authority/releases/<release-id>/authority/bundles/` | Installed `.otapp` trees, including private apps; the release also carries the catalog and host assets |
| Frame: `/var/lib/otrios-authority/state.apps/` | Durable app checkpoints and journals, separate from bundles |
| Frame: `/var/lib/otrios-authority/state.presentation.flex` | Latest complete Monitoring presentation |
| Frame: `/var/lib/otrios-dashboard/` | Disposable native release cache and `last-good.json` |
| Frame: `/run/otrios-direct-console/otrios/` | Ephemeral native readiness reports, update sockets and first-frame capture |
| Frame: `/root/.bashrc` and `/root/.inputrc` | Interactive SSH prompt, aliases and completion preferences |
| Mac: `~/Library/Application Support/OTriOSDashboard/` | Private builder config, source inbox, build outputs, signing publisher and signed release staging |

`/Applications` is the proposed installed-bundle namespace. The
[Spaces RFC draft](docs/rfcs/0027-spaces-and-app-data.md) replaces the earlier
`/PrivateApplications` proposal with Space membership and audience policy.
`/Applications` is **not a live Linux directory yet**; use the release-store path
above to find installed bundles today. Spaces are not implemented yet.

With a local `frame` SSH alias configured, connect and inspect the services with:

```sh
ssh frame
systemctl status otrios-authority otrios-dashboard-direct otrios-tunnel
journalctl -u otrios-authority -u otrios-dashboard-direct --since today
cat /var/lib/otrios-authority/releases/current.json
```

Start with [the PoC plan](docs/POC.md), [architecture](docs/ARCHITECTURE.md), and [development instructions](AGENTS.md). Follow the [contribution instructions](CONTRIBUTING.md) for scoped changes and review.

## Licensing

The runtime uses LGPLv3 only; interfaces, SDK, documentation, and tooling use Apache 2.0. These are path-specific licenses, not a choice of either license for all files. Separate proprietary hardware, firmware, and accelerator implementations are an intended integration model, subject to the applicable licenses. See [the license boundary](LICENSE.md).
