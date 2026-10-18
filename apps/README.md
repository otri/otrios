<!-- SPDX-License-Identifier: Apache-2.0 -->
# Apps

Each Home Screen entry is becoming an app package. An app owns its UI description,
state schema, behavior and local assets. The launcher opens references; it does
not import each app's state, UI implementation or executable instance.

```text
apps/
  home/                         # Launcher: UI and navigation references
    app.rs                      # Typed build-time manifest
    ui.rs
  component-playground/
    app.rs                      # Identity, entry refs, dependencies, migration status
    ui.rs                       # Canvas / Text / Superellipse / Circle + selectors
    state/schema.rs             # Stable field IDs and defaults
    behavior/main.wat           # Pure event reducer; packaged as behavior.wasm
    package/                    # Generated, ignored; never hand edit
      app.flex
      ui.flex
      behavior.wasm
  layout-lab/
    app.rs
    ui.rs
    state/schema.rs             # Its own document, unrelated to Playground history
    package/                    # No behavior binary needed for standard setters
  input-lab/                    # Migration inventory; existing demo still works
  animation-lab/
  three-d-lab/
  monitoring/                   # Private authority capability remains mandatory
```

Add `assets/` for app-owned fonts/images/blobs and `tests/` for local fixtures as
needed; empty placeholder directories are unnecessary. Large assets should become
immutable content-addressed dependencies, rather than CRDT scalar values. Keep
app-specific code here. A reusable component or capability contract moves into
the SDK only when it is shared; apps must not reach into sibling app internals.

`app.rs` is source authoring, **not the runtime manifest format**. `canvas-pack`
compiles it and each UI description into FlexBuffers. The source-level dependency
list names local files plus the shared authoring SDK contract. Wasm imports are
recorded separately in the UI graph, with content digest, ABI and permitted field
IDs. This avoids another manifest parser during the prototype. A future external
app packager should consume the same binary contract without knowing Rust.

`tools/canvas/authoring.rs` is shared build tooling. `otrios-interfaces::canvas`
defines permissive contracts. `otrios-core::canvas` owns the shared LGPL runtime:
selector evaluation, responder routing, guest execution, CRDT validation and
mount lifecycle. The web/native hosts know neither Playground nor Layout Lab
behavior. The packer has an explicit build registry; adding an app currently
requires registering its source there and adding a launcher navigation reference.

## Run the isolated prototype

With the repository's Rust 1.90 / wasm32 and wasm-bindgen toolchain available:

```sh
bash scripts/build-canvas-prototype.sh
target/debug/canvas-server --state target/canvas-state
# Open http://127.0.0.1:8894
# A second process uses the same protocol and behavior binary:
target/debug/canvas-native ws://127.0.0.1:8894/stream
```

The server accepts only loopback binds. Its exact Origin defaults to the bind URL;
use that URL rather than `localhost`. Client documents and guest memory stay in
RAM. The authority persists app documents under `--state`; use a different folder
for independent test authorities. This is separate from the deployed dashboard.
The probe changes Playground's level and checks authority acknowledgement; it
runs headless and does not take over the frame's display.

After changing UI or WAT sources, rerun `canvas-pack` and select **Reload app
package** (or leave/reopen the app). The server reads the new UI/code on mount;
the host executable is unchanged and the stable State reference is preserved.
Schema changes are rejected pending an explicit migration. Active app instances
are not force-replaced mid-gesture. Automatic registry notifications, rollback of
rejected remote mounts and production signed app admission remain follow-up work.
The prototype retains at most eight 64 KiB behavior byte entries in RAM; it does
not install another persistent browser cache or change the live bounded caches.

## Migration order

| App | Current boundary | Next migration |
| --- | --- | --- |
| Home | Independent Canvas UI + empty State root | Catalog of admitted app references |
| Component Playground | Independent UI/State + loadable Wasm reducer | Reuse existing ephemeral trail service as an explicit capability |
| Layout Lab | Independent UI/State; declarative setters | Nested components and reusable templates |
| Input Lab | Legacy adapter manifest | App-owned document fields; shared keyboard/input capability |
| Animation Lab | Legacy adapter manifest | App-owned intent; separate local presentation state/clock |
| 3D Lab | Legacy adapter manifest | App-owned scene/intent; renderer capability and mesh assets |
| Monitoring | Legacy adapter manifest | Separate protected UI/data imports; server-issued read capability |

The legacy apps still use the working dashboard shell. Their manifests inventory
that dependency rather than pretending they have isolated state. Monitoring must
never become public by changing a manifest: authority-side admission and data
policy remain required. App packages cannot grant themselves capabilities.

For each migration, prove independent subscription/persistence, retained native
and web behavior, input capture cleanup, and no state or executable-instance leaks
across navigation. See RFC 0004 and `scripts/check-canvas-prototype.sh`.
