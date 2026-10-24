<!-- SPDX-License-Identifier: Apache-2.0 -->
# Apps

Each Home entry is an independent app package. It owns its primitive UI,
state schema, behavior and local dependencies. Home passes UI/State references;
it does not import another app's document or executable instance.

```text
apps/
  home/                         # Admitted app launcher
  component-playground/         # Controls, bounded Wasm reducer, ephemeral trails
  layout-lab/                   # Reusable components and five-minute signal
  input-lab/                    # Shared text registers; local editing/keyboard
  animation-lab/                 # Shared intent, local animation clock
  three-d-lab/                   # Shared intent, local tetrahedron presentation
  monitoring/                   # Protected UI and replaceable read-only chart view
  catalog.flex                  # Generated public catalog, ignored
```

An app generally contains `app.rs`, `ui.rs`, `state/schema.rs` and optional
`behavior/`, `assets/` or tests. Its generated, ignored `package/` contains
`app.flex`, `ui.flex` and optional `behavior.wasm`. Do not hand-edit generated
artifacts or add empty dependency directories. The packer has an explicit build
registry; register a new app and add a launcher destination.

Shared authored controls, including the keyboard, live in
`tools/canvas/components.rs`.

`app.rs` and shared `tools/canvas/authoring.rs` are build-time authoring. Runtime
contracts live in Apache-licensed `otrios-interfaces::canvas`; interpretation,
CRDT validation, mount lifecycle and guest execution remain in LGPL
`otrios-core::canvas`. A reused runtime implementation should not be cloned into
an app or permissive SDK.

## Delivery and policy

`canvas-pack` emits all seven packages and `apps/catalog.flex`. Catalog entries
bind app identities and UI/State references to UI hashes; the UI binds optional
behavior bytes and permitted field writes. Status/dependency strings do not grant
capabilities. UI v2 supports typed, bounded fields and declared operations;
custom reducers still use the small, separately versioned Wasm ABI v1.

The signed release publishes the generic browser/native hosts, catalog and public
font resources. App UI/code bytes are retained in its signed `authority/apps/`
tree and delivered by admitted mounts, not copied into the public module store.
Monitoring authorization precedes its UI/code/state payloads. The public Home
catalog may name Monitoring without exposing its private package.

Browser and native use one app-scoped transport, `/apps/stream`, with browser
permissions derived from the existing session and trusted native access derived
from its private listener. Public devices and the frame share the same intended
public roots. Clients keep app documents in RAM; only the authority persists them.
Code/blob caches retain their bounded budgets; touch trails remain ephemeral.

Input Lab stores text in shared atomic typed registers, not a per-character text
CRDT. Carets, selections, focus, repeat and keyboard presentation stay local.
Animation/3D keep phase and rendering local while syncing intent. Their stress
control is an isolated RAM CRDT workload. Monitoring replaces a protected binary
Layout aggregate without adding telemetry history to app documents.

## Build and validation

```sh
bash scripts/build-dashboard-web.sh
# With the authority dependencies enabled:
cargo run -p otrios-dashboard --bin dashboard-server --features canvas,server -- \
  --apps apps --web-dir web/canvas --bind 127.0.0.1:8876
```

The browser bundle keeps the operational filename
`pkg/otrios_dashboard.js`/`otrios_dashboard_bg.wasm`, but is built from
`otrios-web-host` and exports CanvasApp plus a generic CanvasPresentation.
It does not export the former Dashboard or bundled MotionPresenter adapters.
The live native host uses the same core Canvas runtime with direct GLES.
`canvas-native` remains a headless process probe, not proof of physical display
or touch acceptance.

The production cutover starts fresh app roots, as authorized. It does not mirror
or silently convert the former global room. Logical state references remain
stable within the new schema; incompatible schema changes need an explicit
migration or new reference. Documents and transport are bounded; history
compaction and general collection/Merkle subscriptions remain future work.

Run `scripts/check.sh`, the Canvas checks, publisher/resource tests and browser
checks. Verify independent subscriptions, persistence-before-acknowledgement,
private admission/revocation, stale generation rejection, reconnect with RAM
edits, and the real native renderer separately. Read the
[modern app pattern](../docs/MODERN_APP_PATTERN.md) and
[RFC 0004](../docs/rfcs/0004-canvas-behavior-and-app-packages.md) for current limits.
Implementation and deployment acceptance are distinct.

## Declarative UI authoring

The source builder keeps relationships together. This is the Home launcher code:

```rust
button("Component Playground")
    .navigate_to(Destination::app("component-playground"))
    .frame([55., 230., 550., 120.])
```

Each app's `screens()` factory declares its screen documents and State bindings.
Screen/package identity is separate from navigation names. Home declares its
children with `Destination::app("layout-lab").route("layout-lab")`; Layout Lab
declares `Destination::app("layout-lab/signal").route("signal")`. The second value
is the relative name owned by that UI. Ordinary links, including Home buttons,
omit `.route(...)` and target the same stable identity from anywhere.

The packer walks named edges from the first catalog screen (Home at `/`) and
writes resolved paths into the catalog. Browser and native use those paths.
Renaming Home's edge to `layouts` yields `/layouts/signal` with no change to the
child document or package identity. UI-only publication recalculates descendant
paths, and active child mounts retain their state and local interaction.

Screens may explicitly share the same State within one app and admission policy.
`Destination::app(...)` resolves the destination's admitted State binding;
explicit UI/State bindings remain supported by the runtime contract. Home declares
its versioned State reference in its own manifest. Back opens the parent path,
then the entry screen at the root.

The compiler creates the button's Canvas, rounded background, text, semantic label,
responder and navigation-table entry together. There is no manually paired
`button(..., 1)` / `action: 1`. The shared host handles generic path/history
plumbing; app-specific names live in the owning app's UI.

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
resizing. Layout v3 adds explicit row/column screen compositions with preferred
and minimum component sizes. Keys name existing instances and
`.with_composition(wide, tall)` arranges the same tree for each viewport.
`.with_touch_pad("key")` resolves a named control without a numeric handler index.
These identities remain scoped to one compiled UI. See
[RFC 0012](../docs/rfcs/0012-modular-screen-composition.md).

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

## Independent UI updates

All apps use the same package factory and update contract. See
[RFC 0013](../docs/rfcs/0013-app-ui-updates.md) for UI-only publishing, qualified
runtime reuse and idempotent activation. UI data can change while compatible
state and behavior stay mounted; executable/schema changes require a full release.

The common publisher also supports `--kind behavior` for an existing guest module;
see [RFC 0014](../docs/rfcs/0014-app-behavior-updates.md). It compiles that app's
Wasm and updates its hash references while reusing the UI layout and host runtimes.

Home uses the bounded [reorder presentation](../docs/rfcs/0015-editable-home-reflow.md):
`.with_reorder(...)` maps stable position fields to authored component keys, and
`wrap_cards(...)` reflows those components without changing their structure.
Its local Edit/Done action controls dragging and wiggle; shared writes contain
only a moved card's position. The renderer owns interpolation and its clock.

Nested screens follow the same incremental publishing contract:
`--app layout-lab/cards --kind ui` rebuilds only that UI document and its catalog
hash, retaining sibling documents and the qualified host runtime. Adding/removing
screen declarations changes the catalog contract and requires a full build.
