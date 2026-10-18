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

`otrios-web-host` is the Canvas-only browser adapter. Its generated
`web/canvas/pkg/host.js`/`host_bg.wasm` bundle excludes the old Dashboard and
MotionPresenter exports. New work follows the [modern app pattern](../docs/MODERN_APP_PATTERN.md)
and its retirement gates rather than extending the old shell.

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

Validation evidence and prototype resource limits are recorded in [canvas-apps-validation.md](../docs/experiments/canvas-apps-validation.md).

## Declarative UI authoring

The source builder keeps relationships together. This is the Home launcher code:

```rust
button("Component Playground")
    .navigate_to(Destination::app("component-playground"))
    .frame([55., 230., 550., 120.])
```

The destination owns both UI and State refs. The compiler creates the button's
Canvas, rounded background, text, semantic label, responder and navigation-table
entry together. There is no manually paired `button(..., 1)` / `action: 1`.
A custom State input will need an explicit destination-ref constructor; the
current convenience constructor uses the app's default UI/State pair.

Bindings and event declarations follow the same pattern:

```rust
text(LEVEL.percent()).position([440., 180.]).font_scale(3.)
rounded_rect(ACCENT)
    .frame([0., 22., 520., 36.])
    .width(LEVEL.mapped(32., 520.))
canvas(slider_primitives)
    .frame([80., 260., 520., 80.])
    .on_drag("Linear slider", Event::slider(LEVEL).reduced_by(SET_LEVEL))
```

`LEVEL` is declared once in the app's state schema. `SET_LEVEL` names the versioned
Wasm event ABI in `behavior/events.rs`. The compiler interns repeated typed
field/projection bindings, assigns control indices and emits all responder tables.
Layout Lab's standard setters omit `reduced_by`; no custom code is required.

There are deliberately separate identity domains:

- **Node IDs** identify retained primitives for input capture/semantics in one
  compiled UI. They are not paint priorities. Sibling order determines painting;
  hit testing visits the reverse paint order.
- **Navigation indices** join compiled navigation responders to their destination
  refs. **Control indices** join event responders to their handlers. The builder
  generates both, and their enum variants keep the domains distinct.
- **State field IDs** are durable schema identities. **Behavior event IDs** are
  guest ABI identities. These remain explicit named constants; changing/reusing
  them needs compatibility or migration work.

Generated UI IDs follow deterministic tree traversal. They are stable for the
same compiled description, but inserting/reordering nodes can change them on
rebuild. They are scoped to the UI activation generation; never persist them as
app-state references. The Canvas invalidates stale input/capture on replacement.
Persistent logical UI keys for editing a UI graph itself remain separate work;
this builder does not claim identity preservation across structural edits.

The builder is source authoring, not a new host toolkit. Its output is still the
same FlexBuffers primitives, selectors, responders and navigation data. Renderers
consume that data and do not execute these Rust builder functions. Frames define component design coordinates; pins and springs handle parent
resizing. Stack layout and intrinsic sizing can be added independently of these
identity/binding improvements.

## Pins, springs and struts

Reusable controls can define their local design canvas with
`component([width, height], children)`. Placing that component in a different
`.frame(...)` preserves its original child design reference. Children express
resizing directly:

```rust
rounded_rect(CARD).pin_edges(0.)
text("Title").pin_left(24.).pin_right(72.).pin_top(16.)
circle(ACCENT).frame([0., 0., 24., 24.]).center_x().center_y()
canvas(children).frame([24., 64., 412., 88.]).spring_width().spring_height()
```

One edge pin keeps size; opposite edge pins stretch between gaps. Centering keeps
size. Springs absorb size changes using the original design lengths; fixed
segments act as struts. Rules are serialized in layout v2 and resolved by the
shared runtime, including input geometry. Fixed v1 layouts remain supported.

`layout-lab/components.rs` defines one reusable card; two instances resize with
the same app state while retaining different instance frames. Button backgrounds
and labels also use pins/centering. See [RFC 0005](../docs/rfcs/0005-pins-and-struts-layout.md)
for weighting, size limits, binary compatibility and the current surface scope.

For the full six-segment spring mask, use `.resizing(Resizing { horizontal:
AxisResize::Springs { leading: true, length: false, trailing: true }, vertical:
AxisResize::Fixed })`. Named fields preserve the relationship without a numeric
bitmask in authored UI code. With 20/80 design margins, these two margin springs
share added width in a 1:4 ratio; `.center_x()` expresses exact centering instead.
