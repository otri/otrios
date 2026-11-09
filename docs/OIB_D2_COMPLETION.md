<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB D2 completion

Date: 2026-11-09. Status: **selected; implementation in progress, not deployed**.
Branch: `feat/oib-d2-completion`, from `39156f2`.

The original repository remains on `main`. Work is isolated in the D2 worktree.
The local authoring prototype was checkpointed in `6c954f0` before the format
cleanup. The live Frame/Web release still uses state protocol v28.

## Selected clean break

The user approved discarding existing PoC data on 2026-11-09. Initialize fresh
OIB collections for this cutover. No predecessor migration, old checkpoint reader
or preservation of existing values, identities or overrides is required. A fresh
collection has a fresh document epoch, so a delayed operation from discarded data
cannot target a replacement object. A signed code rollback can initialize fresh
data compatible with that release.

Keep ordinary runtime guarantees: app-value ownership, causal conflicts and
observed Reset, atomic publication, durable receipts/restart, input safety,
signed class/field/action admission, resource bounds and release-control v1.
Disposable data does not relax these guarantees.

The selected cleanup is:

- One authored graph for definitions, instances, slots, geometry and connections;
  derive the application layout, semantics, responders and object mappings.
- Full typed property addresses as causal keys and wire identities. Compiler
  handles and renderer slots are temporary resources.
- Palette definitions and binding/default presets, with no fake live objects or
  empty inspector placeholders.
- One bounded structural editing API, with an observed structural generation;
  retain independent causal contexts for literal properties.
- Fresh initialization instead of predecessor hash allowlists and legacy readers.

Reuse the existing Merkle library, layout/render/input engines, retained tooling,
transport and signed publication workflow. D3 typography/content, images, XR,
general transactions and complete UI transfer remain separate work. Continue to
refuse OIB data/UI transfer until a complete portable format is implemented.

## Implemented format foundation

This branch now requires package feature and seed format `otrios-oib-v2`, plus
`otrios-oib-designer-v2` for Designer metadata.
The app schemas are `oib-designer-state-v2` and `live-ui-state-v2`; deployment must
initialize fresh collection identities before opening them. Old OIB
packages/checkpoints are rejected. State protocol v29 is the unreleased
candidate in Rust, browser session code and publication tooling. The opening
release-control v1 envelope is unchanged.

OIB edits carry a 32-byte document epoch before their sequence number. The epoch
binds the collection incarnation, scope, UI reference, logical graph ID and
semantic schema/class/property/action contract; it survives a restart of
the same collection and changes on fresh initialization. The authority checks it
before allocation or receipt replay. Clients capture that epoch when queuing an
intent; restoring RAM intents against a different mounted document discards them
instead of substituting the new epoch. An ordinary presentation update with the
identical signed OIB source retains the epoch. Graph body bytes are separate
from semantic admission, but D2 still rejects a changed signed baseline during
configuration and requires a fresh collection. Compatible baseline adoption is
explicit L1 work in [RFC 0036](rfcs/0036-upgradable-oib-component-libraries.md);
equal semantic admission alone never authorizes a new body.

Property changes and inspector actions carry `PropertyAddress`, including the
complete occurrence ancestry. Conflict status stores these same addresses.
Numbers used by local compiler tables do not identify durable registers or wire
edits. Merkle property paths are direct tuples:

| Scope | Path |
| --- | --- |
| Definition | `[4, branch, 0, definition, node, property]` |
| Occurrence | `[4, branch, 1, instance, reference_count, references..., node, property]` |

Branch 1 is the causal draft; branch 2 is the active value projection. The existing
reference-path ceiling is eight, so these paths fit the Merkle depth ceiling of
16. The existing fanout, node, edit and byte limits still apply. Property schema
hashes bind the app scope and canonical typed address under
`OTriOS\0oib-property-v2\0`.

There is one canonical structural record at `[4,4]`, `DocumentSource`, containing
shared definitions, true presets, durable instance descriptors, the shared
monotonic identity cursor, baseline role mappings, placements and typed app
connections. The old independent allocation/cursor,
definition, placement and connection records have been consolidated. Structural
source is bounded to 24 KiB, including the existing 16 KiB definition, 4 KiB
placement and 4 KiB connection sub-budgets; the combined resident authored/causal
budget remains 64 KiB.

`InstanceRecord` contains no renderer slot. The runtime derives slots from resident
identities. New object IDs follow the highest admitted initial identity; there is
no reserved object-ID range or object-ID-to-cell arithmetic. Property handles may
be reused after reclamation; delayed edits retain their old semantic address.
The active closure hashes the active graph projection, excluding the allocation
cursor and inactive resident bookkeeping. Thus an invalid draft cannot change a
pinned active closure. Surviving causal registers remain unchanged by reclamation.

The canonical signed seed serializes only its format and graph. Layout,
selectors, handlers, cells, mappings and instance renderer slots are derived.
The production Designer uses reusable definitions and binding presets: Power is
a palette entry without a phantom initial object. SettingsPanel declares a real
Accessories slot. Legacy allocation decoding and predecessor allowlists are
removed. Tests use canonical fixtures and semantic property addresses; unused
flat/migration fixture artifacts have been deleted.

Untouched shared property registers contain no literal override. They inherit
the current authored default; explicit shared and occurrence values retain their
causal contexts. Changing a default therefore updates untouched instances without
overwriting explicit edits.

## Current structural cutover

The graph compiler and one generation-guarded `EditBatch` are implemented locally.
Definition edits precede presets and hierarchy edits; the complete candidate is
validated and published once. Rejected batches retain the active graph and consume
no identities. Literal property changes use their independent causal contexts and
do not invalidate an observed structural generation. Restore is a singleton batch
and uses baseline roles plus fresh identities for deleted objects.

The backend exercises shared rename/reorder,
exposed forwarding, reference assignments, owned slot insertion/movement/removal,
subtree duplication, placement and admitted app binding/effect reconnection.
Those behaviors and their tests now use the graph compiler. Definition/node
and preset creation/deletion, visual declarations and authored defaults now have
bounded operations. Visible controls create components/presets, add shapes/text
and linked references, remove unused objects, rename designs and instances, edit
literal visual declarations, and connect admitted app fields, including an unbound
preset. Numeric placement, slot operations and shared reorder use the same batch.
Structural inspector drafts retain their captured generation; stale drafts remain
for review instead of silently rebasing. Live qualification remains in progress.
This is **not completed D2 acceptance**.

Source projections contain no predicted renderer IDs. The presented graph supplies
actual object mappings; hierarchy navigation uses semantic classes. Inspector
wheel routing and diagnostics refer to the real retained editor scroll state.

Read-only live projections are explicitly distinguished from durable checkpoints.
A missing private draft branch is legal only in that live projection context;
loading incomplete full checkpoints still fails. Before journal replay, the
verified UI can warm transient compilation only when the checkpoint root remains
identical. Initialization or a compatible chrome binding update stays after replay.

## Validation and completion gates

Use Rust 1.90.0 and the committed lockfile. Runtime code/tests remain LGPL core;
contracts, app descriptions, tooling and docs remain Apache-2.0. No dependency or
unsafe-code addition is needed for this cleanup.

Current local focused evidence (before signed deployment): graph compiler 11,
production fixture 14, runtime 9, format/epoch 5, definition authoring 11, hierarchy
authoring 8, navigation 11, property sources 7 and property editor 13 cases passed.
All 31 OIB integration cases passed, retaining the 24/64 churn iterations. The
editor's 41 cases passed across the main run and one focused corrected-viewport
rerun; the additional unbound-preset source-choice case also passed. The remaining
245 core library cases, three collection-reset cases and 48 focused Node cases
passed. Workspace all-target Clippy with `-D warnings` passed. The repository-wide
check and final long authority restart qualification are running; do not count
them or Frame/Web acceptance as passing yet.

Historical development evidence at foundation commit `8548f43` (before the
canonical compiler/edit-batch cutover; these are not results for the current tree):

- OIB causal properties, lifecycle, invalid draft publication and gesture tests:
  31 passed during this increment; obsolete migration-only cases were removed.
- Definition authoring: 11 passed. Designer fixture: 14 passed. Designer runtime:
  15 passed. New format/epoch tests: five passed.
- The hierarchy authoring suite passed eight cases, including a three-child slot
  cardinality rejection that verifies no identities are consumed. Navigation and
  property-source suites passed 11 and seven cases, respectively.
- A freshly built `dashboard-server` passed both `spaces_process d1::d2_` tests:
  two passed, none skipped. These cross a real authority process and cover races,
  lost accepted/rejected replies, exact replay, kill/restart, read-only projection,
  stale order operations and unchanged app/other-app values. The separate real
  authority lost-reply/reclamation/retired-path restart test also passed.
- Workspace Clippy passed with all targets, the server/canvas/host-sync/Merkle
  features and `-D warnings`. The 41 focused OIB library cases passed across the
  initial run and focused reruns after replacing obsolete numeric-ID assumptions.
- Shared session/update/text-input Node suites: 36 passed, including codec agreement
  and upgrade discovery when state negotiation/recovery is unavailable.

These are local development checks, not Frame/Web acceptance or Linux shared-memory
IPC qualification. Finish the remaining cutover, required repository checks, signed
ARM/Wasm build and deployment, fresh-collection activation and focused Frame/Web
acceptance before opening a PR. Keep a compatible signed code rollback available.
After user acceptance, follow the existing builder-maintenance workflow.


## Fresh-collection operational cutover

Publishing signed code does not replace app collections. After the new signed
catalog is active, use the existing authenticated Space Admin `DeleteData`
management command with `remove: false`, naming the exact displayed collection
and current registry generation. The reset adopts the currently installed,
verified app policy/schema and publishes its fresh collection plus the old
collection's deletion record in one registry decision. It retains the app's
position, Space membership and audience. A newly private policy cannot be adopted
into a public Space. No old app payload is decoded or migrated.

Reset only the affected `oib-designer` and `live-ui` collections in their existing
Spaces. Their new identities cause mount resumption to start at zero and old
bindings/intents to fail. Verify the resulting schema, fresh collection identity,
native/browser mounting and the normal signed release readiness reports. This is
the authorized destructive cutover; it requires no data preservation inventory.

The compatible signed v28 rollback predates schema adoption during reset. If a
rollback is needed, activate its matching signed code/server first, then use the
existing Admin `DeleteData { remove: true }` followed by `Attach` for each affected
app, each with a fresh observed registry generation. This explicitly initializes
its installed v1 schema and does not attempt to open v2 checkpoints. Preserve
membership, audience, signing roots and all unrelated app collections.
