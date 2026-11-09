<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB D2 completion

Date: 2026-11-09. Status: **implemented, deployed and focused live-qualified; awaiting user review**.
Branch: `feat/oib-d2-completion`, from `39156f2`.

This records the dated D2 v29 qualification. [Library L1](OIB_LIBRARY_L1.md)
subsequently deployed state v30 and supersedes its active-release statements.

The original repository remains on `main`. Work is isolated in the D2 worktree.
The local authoring prototype was checkpointed in `6c954f0` before the format
cleanup. Implementation commit `5aa2a39` is deployed as signed release
`f642416df9b7754d39aef8b62cbb592783ea6df5989f965a45c7fd6e392ac18b`,
using state protocol v29 and release-control v1.

## Selected clean break

The user approved discarding existing PoC data on 2026-11-09. The cutover initialized fresh
OIB collections; the resets are complete. No predecessor migration, old checkpoint reader
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
use the fresh collection identities initialized for this cutover. Old OIB
packages/checkpoints are rejected. State protocol v29 is the deployed
version in Rust, browser session code and publication tooling. The opening
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

The graph compiler and one generation-guarded `EditBatch` are deployed.
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
for review instead of silently rebasing. The focused live checks below qualify
this bounded scope; D3/D4, complete OIB transfer and XR are separate. Signed
library loading subsequently shipped in [L1](OIB_LIBRARY_L1.md).

Source projections contain no predicted renderer IDs. The presented graph supplies
actual object mappings; hierarchy navigation uses semantic classes. Inspector
wheel routing and diagnostics refer to the real retained editor scroll state.

Read-only live projections are explicitly distinguished from durable checkpoints.
A missing private draft branch is legal only in that live projection context;
loading incomplete full checkpoints still fails. Before journal replay, the
verified UI can warm transient compilation only when the checkpoint root remains
identical. Initialization or a compatible chrome binding update stays after replay.
The normalized binding cache is keyed by the immutable `[4,0]` leaf hash and exact
admitted UI identity; it never changes root or wire bytes. OIB journals use the
existing atomic checkpoint rotation at 256 KiB instead of the ordinary 4 MiB
threshold, bounding accumulated graph replay without increasing test deadlines.

The subsequent [incremental presentation cleanup](experiments/oib-incremental-presentation-2026-11-10.md)
retains admitted typed Blueprints across graph and placement transforms. Authored
and expanded metadata and baseline digests reuse exact binding/source leaf hashes
and admitted UI identity. Proven confirmed Color-only changes preserve layout and
input identities; independent per-draw paint retains GPU geometry. Width, text,
structural and unproven dependencies retain the full compilation path.

Revision tooling authenticates a predecessor's package envelope, complete file
closure and publisher signature without admitting its obsolete UI payload.
The resulting metadata-only revision record cannot be installed or executed.
Current package inspection, signature verification and host admission remain
required for the new candidate; no old OIB reader or feature allowlist was added.

## Validation and live qualification

Rust 1.90.0 and Cargo.lock remain pinned. Runtime code/tests stay LGPL-3.0-only;
contracts, app descriptions, tooling and docs remain Apache-2.0. No dependency,
unsafe code, service, trust root or identity was added.

Actual local commands used the existing private toolchain/target environment:

- `cargo test -p otrios-core --features canvas,merkle,signed-bundles --lib --locked --offline`:
  **299 passed**, including binding-cache and predecessor-authentication tests.
- `cargo test -p otrios-core --features canvas,merkle,signed-bundles --test '*' --locked --offline`:
  **414 reported passed**, including the corrected 69-case Canvas suite.
- `cargo test --workspace --exclude otrios-core --features otrios-dashboard/server,otrios-dashboard/canvas,otrios-dashboard/host-sync,otrios-core/merkle --locked --offline`:
  **77 passed** in the other workspace targets.
- Workspace all-target Clippy with those server/canvas/host-sync/Merkle features
  and `-D warnings`, formatting and whitespace checks passed.
- The check script's Python suites passed **73 cases**, the independent portable
  fixture verifier passed, signed bundle installation passed **3 cases**, and
  focused shared-session/input/worker/transfer/cache Node suites passed **75 cases**.

`bash scripts/check.sh` initially stopped at a stale test that inspected derived
Frame Focus data on raw canonical source. After normalizing that fixture, all
69 Canvas cases passed and the remaining commands above ran separately. The
script is not reported as a single uninterrupted passing run. Process tests can
return early without `OTRIOS_SPACES_SERVER`; their ordinary test-engine totals
alone are not process acceptance.

With a freshly built `dashboard-server` explicitly supplied through
`OTRIOS_SPACES_SERVER`, both `spaces_process d1::d2_` cases passed. The final
`d1_lost_reply_receipt_reclamation_and_retired_path_survive_real_authority_restart`
case passed with all **24 cycles**, exact lost-reply replay, retired-path discard,
checkpoint advancement, a retained journal below 512 KiB and final kill/restart
under the unchanged **10-second** observation deadline. The cache-key regression,
16 unified-batch cases and other focused permission/conflict/restart cases passed.
Two earlier debug recovery runs exceeded the deadline; those failures led to the
binding cache and bounded OIB checkpoint policy and are not counted as acceptance.
These are Canvas authority-process checks, not Linux shared-memory IPC qualification.

The signed ARM/Wasm build, package composition, transfer and receiver verification
passed. The actual old signed Live UI package could supply authenticated revision
metadata for the v2 bump while strict current-host verification still rejected
its obsolete UI. Signed release activation used the established deploy script and
existing publisher key. A verified compatible code-only rollback archive is held
privately; no app-data backup or migration was performed.

Focused live acceptance on release
`f642416df9b7754d39aef8b62cbb592783ea6df5989f965a45c7fd6e392ac18b`:

- Only Live UI and OIB Designer were reset through authenticated Admin requests
  with exact observed collections/generations. Registry generations 74/75 adopted
  the installed v2 schemas. Other apps, membership and audience were retained.
- The real browser/Wasm client mounted Designer, created a component/preset with
  no phantom instance, edited design width **320 → 400**, then deleted that scratch
  definition. Revisions **1 → 2 → 3 → 4** kept roots **20/30**, the original
  18 property displays and slider values **40/70**. Live UI also mounted its own
  fresh v2 collection without an error.
- Physical touch on Frame opened Designer at revision 3, with confirmed active
  and presented generations equal. The actual native renderer remained GLES2,
  rotation 90, with its calibrated touch device and existing scanout unchanged.
- A current ARM Linux client inserted Power into the production **Accessories**
  slot, moved it to the canvas, then removed it at revisions **4 → 5 → 6 → 7**.
  All four business values stayed equal and the original two active roots remained.
- Readiness/health/service checks passed; Monitoring resumed successfully. No
  private pixels were exported. The release-control v1 boundary stayed intact.

[Acceptance summary](experiments/data/oib-d2-canonical-2026-11-09/acceptance.json)
records these bounded facts. Await user review before accepted-work builder
maintenance. Signed library loading subsequently shipped in
[L1](OIB_LIBRARY_L1.md), under [RFC 0036](rfcs/0036-upgradable-oib-component-libraries.md).

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
