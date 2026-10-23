<!-- SPDX-License-Identifier: Apache-2.0 -->
# Modular screen layout validation — 2026-10-23

Home, Component Playground and Layout Lab now compose the same reusable
components into 1280×800 landscape and 800×1280 portrait scenes. Component
structure, styling, control identities and state bindings are shared between
profiles. Other apps retain their legacy logical scene and fit uniformly.
See [RFC 0012](../rfcs/0012-modular-screen-composition.md) for the contract.

## Checks

- `RUST_TEST_THREADS=1 bash scripts/check.sh`: passes (164 Rust tests,
  34 Python checks, formatting and strict workspace Clippy).
- `node --test scripts/test-dashboard-session.mjs scripts/test-dashboard-monitoring-session.mjs scripts/test-dashboard-artifact-cache.mjs scripts/test-app-catalog.mjs scripts/test-canvas-worker.mjs scripts/test-viewport.mjs scripts/test_gpu_scene.mjs`:
  all 43 tests pass.
- `cargo build -p otrios-web-host --lib --release --locked --offline --target wasm32-unknown-unknown`
  and the browser binding build pass; `node --test scripts/test-canvas-host-exports.mjs`
  passes against that Wasm artifact.
- In the ARM build container, `cargo clippy --workspace --all-targets --all-features --target armv7-unknown-linux-gnueabihf --locked --offline -- -D warnings`:
  passes, including the native GLES/touch host. Projection/touch unit tests cover
  all four rotations, portrait surfaces and legacy scene fitting.
- `node scripts/test-responsive-browser.mjs --url http://127.0.0.1:18877 --output target/responsive-browser`:
  passes in real headless Chromium using software WebGL and a disposable local
  authority. It checks Home column changes, pointer navigation, stable control
  identities, shared slider edits and ink between clients in different
  orientations, semantic focus retention, and input after resizing mid-drag.

The local authority used a disposable signing key and a placeholder native
artifact solely to serve a complete browser test fixture. It is not a deployment
artifact. These checks preceded the live activation recorded below. Physical
touch, panel orientation and device GPU performance remain separate acceptance
items.

## Browser captures

These are actual Chromium captures from the passing acceptance run.

| Screen | Landscape | Portrait |
| --- | --- | --- |
| Home | [Capture](modular-home-landscape.png) | [Capture](modular-home-portrait.png) |
| Playground | [Capture](modular-playground-landscape.png) | [Capture](modular-playground-portrait.png) |
| Layout Lab | [Capture](modular-layout-lab-landscape.png) | [Capture](modular-layout-lab-portrait.png) |

![Playground landscape](modular-playground-landscape.png)

![Playground portrait](modular-playground-portrait.png)

## Faster iteration

The current watcher freezes, tests and rebuilds both runtimes on every source
change. Cargo caches avoid recompiling unchanged dependencies, but the watcher
still invokes the whole pipeline. Publication always makes a complete signed
manifest; the deployer compares the installed server bytes and restarts the
server only when those bytes changed. Browser upgrades currently reload the
page, and native upgrades restart the renderer after readiness checks.

A useful next increment is to classify changes and reuse verified artifacts:

| Edit | Build work | Activation |
| --- | --- | --- |
| App UI/component arrangement | Run `canvas-pack`; reuse compatible host binaries and browser Wasm | Sign a new manifest/catalog and activate; retain durable app state |
| Browser CSS/JavaScript | Reuse native and Wasm artifacts | Sign and activate the browser asset change |
| Guest behavior `.wat` | Rebuild the affected guest/package | Reuse compatible hosts; publish the new signed behavior reference |
| Resolver, renderer or host API | Rebuild affected native/Wasm/server targets | Activate the matching code/UI pair |
| State or wire contract | Rebuild matching codecs/hosts and run compatibility checks | Coordinated release; do not treat as a UI-only patch |

The UI-only path must compare runtime/toolchain inputs and require an existing
verified compatible build. Reusing whatever binaries happen to be on disk would
not establish compatibility. Add this path to the existing watcher rather than
starting a second publisher.

The next transport improvement is content-addressed asset reuse across signed
releases. Existing release-scoped browser URLs and native release directories
cause unchanged assets to be fetched again when the release ID changes. The
cache helpers already recognize `/objects/<sha256>`, but the active server has
no public object route, so that is not an existing cross-release delivery path.
Reuse must verify each object's size/digest and retain the complete signed
manifest as the atomic activation boundary. Keep private app bytes behind
authorized mounts.

True component hot replacement is a later increment: admit the updated catalog,
wait for a safe gesture boundary, then replace the affected UI mount while
retaining compatible document/focus state. Current upgrades reload the browser
or restart the renderer; they do not patch an individual component in place.
For iPad feedback, show a short release ID alongside screenshots and record the
screen/orientation so each observation maps to the exact deployed build.

## Live deployment — 2026-10-23

Source `c9a378198a35a95326781a5528feff2043cd258f` was built, signed with the
established publisher and deployed to [the iPad origin](https://otrios.otri.ca/).
The complete release was verified before activation. Both the authority and
native last-good pointer now identify
`57c7965bd941bb10243ab0d97a89e0002a38dffbcaa383fd93a0d2cfe7ed5daa`.
Native first presentation and a confirmed Monitoring mount succeeded, with the
accepted 90-degree output rotation and calibrated touch configuration retained.
Authority, tunnel and native services are active. Physical touch/color/performance
acceptance has not been performed remotely.

A 27 MiB rollback archive is retained locally at
`/var/backups/otrios/2026-10-23-231936-before-portrait.tar.gz`.
It contains the previous signed release, binaries, helpers and service units;
it excludes live durable app state so rollback does not erase confirmed edits.
Local frame backups are sufficient for this hobby deployment, per the user's
preference. [Deployment evidence](modular-screen-deployment.json) records the
source/release IDs, native readiness and backup checksum.

A browser over SSH forwarding could not be exercised because the installed SSH
policy prohibits port forwarding; that policy was preserved. The browser
composition acceptance above used the disposable local authority. Public HTTPS
conformance passed all 69 checks against the actual origin: signed public asset
integrity, release/session negotiation, private/admin isolation and anonymous
Monitoring denial. Login was not exercised.
