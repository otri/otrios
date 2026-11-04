<!-- SPDX-License-Identifier: Apache-2.0 -->
# Desktop Studio and bounded canvas authoring

Date: 2026-11-04. Status: implementation in progress.

Desktop Studio presents persistent Layers, runtime canvas and Properties using
Frame Focus's typed hierarchy, drafts, provenance, reset and retained scrolling.
The browser chooses Studio; native Frame chooses Focus. An explicit local
profile control switches presentations. Sidebar sizes and the profile are local
client preferences retained across app mounts; they never enter shared UI data.
Resize keeps authored preview geometry, full property addresses and app bindings.

## Active goal and completion boundary

Deliver Desktop Studio with persistent, resizable hierarchy/canvas/properties
panels, sharing Frame Focus's editing model. Then provide a palette of the
existing Audio/Display panel factories and one simpler reusable Power toggle,
with insertion and reordering inside one admitted canvas container.

This goal is complete when:

- Desktop Studio and Frame Focus use the same selection, typed property drafts,
  provenance, reset and scrolling model; resizing is local and respects input,
  clipping and existing scene/control limits.
- An author can insert any admitted palette factory and move a selected root
  earlier or later using the actual UI. The starting Audio/Display document and
  existing app bindings remain intact.
- Focused qualification demonstrates stable overrides and unchanged app values
  after insertion/reorder, explicit rejection and deliberate retry of competing
  structural edits, and durable order/receipts through reconnect and authority
  restart.
- The completed code is built, signed, deployed to the existing Frame/Web
  environment and checked live, with an available compatible code rollback.
  Actual commands, results and deferred checks are documented.

Full nested/slot/definition authoring, connection editing, reusable application
text controls/content sizing, rich typography, complete portable UI transfers,
images/assets and O3D/XR are outside this goal. The next goal below is planning
only and does not expand the current implementation.

## Narrow D2 contract

This increment admits insertion and ordering of whole signed factory instances
in one container: the presented document canvas. It does not admit nested slots,
reparenting, definition structure mutation, new connections, removal repair or
arbitrary primitives. The palette names the signed factory and its app binding;
it is not a list of newly composable D3 controls.

The container is derived from signed metadata: every admitted factory must be
owned by the document and visually attached to its presented root node. Each
insert specifies the exact factory prototype (so Audio and Display bindings
cannot be confused), optional `before` sibling and observed active generation.
Each reorder specifies an existing live root, optional `before` sibling and the
observed generation. `None` appends. The authority validates the entire request
before changing allocations, identity counters, property registers or the UI.

Structural operations require the observed generation to equal the authority's
active generation and a coherent draft/active graph. Competing requests against
one generation accept at most one change. A stale or inadmissible request is
acknowledged as rejected, with a bounded diagnostic and durable sequence receipt;
it does not retry automatically or overwrite the accepted graph. Refresh the
confirmed order and deliberately retry to rebase. Replayed receipts never allocate
or move twice, including across restart. No structural sequence CRDT is added.

The existing resident allocation list supplies order. Renderer slots remain
immutable per resident, while instance IDs and full relative property keys keep
all existing overrides attached to their occurrences. Projection assigns visual
order from the list without copying or clearing any surviving property register.
Insertion uses a fresh monotonic identity, signed defaults/authored assignments
and the factory's original app bindings. Existing resource caps still apply.

The opening release-control envelope remains v1. New instance-operation variants
require state protocol v28 in both browser and native/server codecs. The resident checkpoint shape is unchanged, but the signed palette-only factory
descriptor requires a rollback runtime that understands it. Retain a verified
compatible runtime baseline; do not treat the preceding v27 binary as compatible
after this descriptor has been persisted. Code rollback never restores old
application data.

Qualification and signed release details will be recorded after live acceptance.


## Next goal: design-agnostic OIB and Components dogfooding

After this active goal is qualified, make OIB independent of any particular app
design and rebuild Test Playground's Components interface as an OIB document.
Use the conversion to expose and close architecture gaps rather than adding
another app-specific renderer or a privileged fixed fixture.

The OIB document should own composition, layout, styling, reusable definitions,
references and instance overrides. Signed contracts admit the available classes,
behaviors and connections; app bindings preserve the app's value ownership.
Choose and freeze a bounded authoring surface for the conversion before coding.
Do not silently fold all of D2, D3 or D4 into that next milestone.

Proposed completion evidence:

- Construct and edit the Components interface through Studio using the ordinary
  document and authoring APIs; no runtime branches keyed to the sample's design.
- Reuse definitions and nested instances with stable overrides, app bindings and
  values through live edits, concurrent edits, reconnect and restart.
- Close the portable-transfer gap before widening OIB into the existing app:
  export/import includes UI definitions, references, overrides and causal/conflict
  metadata as well as confirmed app values.
- Qualify the rebuilt interface on Frame and Web, recording any additional
  authoring/behavior capabilities explicitly selected to support the conversion.

Images/assets and O3D/XR stay separately deferred. Richer application controls,
content sizing and typography remain separately selected stages unless the next
milestone explicitly admits a narrow prerequisite.
