<!-- SPDX-License-Identifier: Apache-2.0 -->
# Live OIB UI

OIB provides Space-owned reusable UI designs, shared edits and per-occurrence
properties. **Live UI** and **OIB Designer** have separate app state and OIB
collections. Space Admins can edit the design; admitted participants receive the
confirmed active result. App values remain independent of UI design properties.
The public contracts are in `otrios_interfaces::oib`, also available through the
SDK's `oib` feature; runtime implementation and tests live in `otrios-core`.

## Designer and authoring

Open **Test Playground → OIB Designer** in the selected Space. Audio and Display
are real initial SettingsPanel instances with linked slider and toggle definitions.
Power is a preset, not a hidden initial instance. SettingsPanel has a real
Accessories slot. The browser defaults to Desktop Studio and Frame to Focus;
selection, scrolling and Edit/Test mode are local. Edit selects preview objects;
Test invokes confirmed app actions.

The visible editor can create components and presets, add shapes, text and linked
references, delete unused objects, rename designs and instances, edit literal
visual and numeric design/placement values, place objects in slots, and select
admitted app fields including unbound endpoints. The bounded SDK additionally
supports creation, deletion, rename, movement, reference forwarding and typed
connections. This is not every inspector or control proposed by
[RFC 0035](rfcs/0035-oib-component-designer.md). Structural drafts retain their
observed generation; stale edits require review instead of silently rebasing.

A signed `Seed` serializes its format and canonical graph. Definitions, presets,
real instances, the next identity, baseline roles, placements and typed connections
form the authored source. Presets are independent of initial instances. Layout,
selectors, handlers, renderer slots and cells are derived; none is a durable fixed
alias identity. Full typed property addresses include occurrence ancestry and
identify causal registers and edits. Deleted objects are recreated with fresh IDs.

A bounded `EditBatch` applies definition edits, then preset edits, then hierarchy
edits, validates the candidate and publishes it once. Restore is a singleton
batch. Structural generation guards structural edits; literal properties have
separate causal contexts. An unset shared register inherits the authored default.
Observed resets clear the appropriate overrides without erasing concurrent or
unobserved edits. Invalid or conflicted drafts retain the previous active graph.
The authority journals accepted edits before publication; activation also waits for
confirmed edits and finished gestures.

The current format requires `otrios-oib-v2` and Designer metadata requires
`otrios-oib-designer-v2`. The fresh collections use `live-ui-state-v2` and
`oib-designer-state-v2`. State protocol is **v29**, with the stable
release-control **v1** opening envelope on the shared WebSocket. A collection
epoch prevents delayed edits from a discarded collection targeting its replacement.
The [D2 work record](OIB_D2_COMPLETION.md) contains the format, implementation
details and complete qualification record.

## Live qualification

The signed release `f642416df9b7754d39aef8b62cbb592783ea6df5989f965a45c7fd6e392ac18b`
from implementation commit `5aa2a39` is mounted by both apps. Authorized fresh
resets of only Live UI and OIB Designer completed at registry generations 74 and
75 with their new schemas. In the live browser, a true component and preset were
created without a phantom instance, literal design width changed from 320 to 400,
and a scratch component was deleted; active revisions advanced from 1 through 4.
Original roots 20 and 30 and slider values 40 and 70 remained unchanged. Physical
Frame touch opened Designer at shared revision 3. Readiness passed at this release;
GLES direct rotation 90 and touch calibration were unchanged.

The core library passed 299 tests, with 414 reported core integration and 77
other workspace cases. The process test engine may return early without an
explicit server, so those totals alone are not real-process qualification. Two
focused D2 real-process tests passed, as did the full 24-cycle lost-reply,
reclamation and final-restart test with its 10-second deadline and new 256 KiB
OIB checkpoint rotation; its cycle count was not reduced. These results do not
qualify the separate Linux shared-memory IPC PoC.

Predecessor package metadata authentication verifies the old package signature
and complete file closure without decoding or executing its old UI. Installation
of the current package still requires strict current verification and admission.

## Boundaries and history

Complete OIB data/UI transfer, D3/D4, images and XR remain deferred. Existing Wasmi
executes app ABI 1; classes and actions remain bounded host contracts. The selected
next boundary is an independent signed data-library L1, followed by component Wasm
ABI L2, described in [RFC 0036](rfcs/0036-upgradable-oib-component-libraries.md)
and [the library boundary](OIB_LIBRARY_BOUNDARY.md). Neither a library loader nor
component Wasm ABI is implemented.

Earlier evidence describes its dated implementations: [Live UI O1](experiments/live-oib-2026-11-02.md),
[O2](experiments/live-oib-instances-2026-11-03.md),
[Designer D1](experiments/oib-designer-d1-2026-11-03.md),
[Frame Focus](OIB_FRAME_FOCUS_D1.md), and
[Desktop Studio](OIB_DESKTOP_STUDIO_D2.md). Those records are not the current
format specification. Documentation and public contracts are Apache-2.0;
runtime code and tests are LGPL-3.0-only.
