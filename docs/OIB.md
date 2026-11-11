<!-- SPDX-License-Identifier: Apache-2.0 -->
# Live OIB UI

OIB provides Space-owned reusable UI designs, shared edits and per-occurrence
properties. **Live UI** and **OIB Designer** have separate app state and OIB
collections. Space Admins edit designs; admitted participants receive the
confirmed active result. Business values remain independent of UI properties.
Public contracts are in `otrios_interfaces::oib`, reexported by the SDK's `oib`
feature. Runtime implementation and tests live in `otrios-core`.

## Designer and libraries

Open **Test Playground → OIB Designer** in the selected Space. Audio and Display
are real initial SettingsPanel instances. Power is a preset without a hidden
initial object. SettingsPanel has an Accessories slot. Both hosts use the same
responsive canvas and Library/Layers/Inspect task dock. Selection, scrolling and
Edit/Test mode are local; Test exercises local sample values without writing shared
app state. Dark/Light currently changes editor chrome, pending authored style modes.

Designer imports SettingsPanel, LabeledSlider and LabeledToggle from the signed
[OIB 2D library](../libraries/oib-2d/README.md). Shell, binding presets, business
connections and grants remain app-owned. Imported structure is read-only; shared
and occurrence literal overrides remain editable. Local wrappers and definitions
use the ordinary bounded authoring operations. Live UI currently retains its
local definitions and independent values.

The editor creates local components/presets, shapes, text and linked references;
removes unused local objects; renames designs and instances; edits literals and
numeric placement; places objects in slots; and selects admitted app fields,
including unbound endpoints. Structural drafts retain their observed generation
and require review after a stale rejection. This is the bounded surface from
[RFC 0035](rfcs/0035-oib-component-designer.md), with D3/D4 still separate.

## Source, identity and publication

A signed seed contains its format and canonical local graph, plus explicit import
locks/bindings and separately transported immutable library bodies when present.
Definitions, true presets, real instances, placements and connections supply
source; layout, selectors, handlers, renderer slots and cells are derived.
Full typed property addresses include occurrence ancestry and identify durable
registers and edits. Deleted local objects are recreated with fresh IDs.

One bounded `EditBatch` applies definitions, presets and hierarchy, validates the
candidate and publishes it once. Restore is a singleton batch. Structural edits
use a structural generation; literal properties use independent causal contexts.
An absent assignment inherits the current authored/library default. Explicit
shared/occurrence edits remain overrides. Observed Reset clears the corresponding
assignment without erasing unobserved concurrent edits.

A library update changes an app's exact signed dependency pin. The authority
checks expected generation/baseline, signatures, export/class compatibility,
retained addresses, local source, complete compilation and aggregate bounds
before durable adoption. Compatible updates retain the collection, semantic
epoch, business data, causal heads and receipts. Invalid candidates leave the
prior document active. Arbitrary signed app-source changes still require fresh
admission. Names, version labels and content hashes never supply authority.

The client pins its complete presented UI and handlers through gestures, focus
and unconfirmed edits. Live leases may coalesce intermediate revisions; durable
journal replay still validates each exact adoption. The normal protocol retains
one acknowledged snapshot, one in-flight offer and the latest authority state;
optional UI history excludes imported library bodies.

Current app features are `otrios-oib-v2` and `otrios-oib-designer-v2`; imported
libraries additionally require `otrios-oib-library-v1`; executable libraries add
`otrios-component-behavior-v1`. Business schemas remain
`live-ui-state-v2` and `oib-designer-state-v2`. State protocol is **v31** and the
shared WebSocket retains the release-control **v1** opening envelope. Release
discovery remains available independently of state recovery.

## Current qualification

The historical L1 signed release `ca310b5ca4a75a43b7891a1492085bfae35a04bf29cddff54d4f8160ea39d8e1`
uses library revision
`3ce95edfa948a4d0299e869c7b7f070e4419fb37fea3f7955669cae724113aba`.
The [L1 record](OIB_LIBRARY_L1.md) includes commands and sanitized evidence for
signed publication, unchanged host hashes during library-only updates, compatible
rollback, held browser input, native Frame mounting and authority restart.
Untouched widths/default colors updated while an explicit 360 px occurrence
width, all four business values, collection identity and semantic epoch survived.
The initial format cutover reset only the two OIB collections; subsequent library
updates and the corrected L1 host deployment needed no reset or data migration.

[L2 qualification](OIB_LIBRARY_L2.md) records the current executable-library
increment: component ABI 1, a signed library-owned Slider, and guest-only behavior
update/rollback with unchanged hosts. The first new behavior export required one
fresh Designer admission. Later compatible revisions retain app values, the
Display Width 420 override and semantic identities.

Runtime snapshot/process checks cover the Canvas authority. They do not qualify
the separate Linux shared-memory IPC PoC. Complete authored UI/data transfer,
D3/D4, images and XR remain deferred. Wasmi executes independent bounded app ABI 1
and component ABI 1 contracts, as described in
[RFC 0036](rfcs/0036-upgradable-oib-component-libraries.md).

Earlier records describe their dated increments: [Live UI O1](experiments/live-oib-2026-11-02.md),
[O2](experiments/live-oib-instances-2026-11-03.md),
[D1](experiments/oib-designer-d1-2026-11-03.md),
[Frame Focus](OIB_FRAME_FOCUS_D1.md), [Desktop Studio](OIB_DESKTOP_STUDIO_D2.md)
and [canonical D2](OIB_D2_COMPLETION.md). Public contracts/docs use Apache-2.0;
runtime code/tests use LGPL-3.0-only.
