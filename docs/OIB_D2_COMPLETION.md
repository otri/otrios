<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB D2 completion

Date: 2026-11-09. Status: **selected; contract and implementation in progress**.
Branch: `feat/oib-d2-completion`, from `39156f2`.

## Disposable development data

On 2026-11-09 the user clarified that existing data does not need preservation.
D2 may introduce a new document/checkpoint format and initialize fresh affected
collections. Predecessor migration, old checkpoint decoding and preservation of
current fixture identities/overrides are no longer acceptance requirements.
Initialize a fresh document/collection epoch so stale operations from a discarded
document cannot target newly created objects.

This changes upgrade policy, not the behavior of the new runtime: ordinary design
edits must still respect app-value ownership, causal conflicts, atomic publication,
durable receipts/restart and local input safety. Keep release-control v1, signature
verification, signed class/field/action admission and explicit resource bounds.
A code rollback may initialize fresh data compatible with its signed schema.

## Proposed cleanup before completing D2

The current prototype extends D1's flat signed-layout lowering. With disposable
data, the recommended boundary is a clean document model instead:

- One canonical authored graph owns definitions, instances, slots, placement and
  typed connections. Compile it into the existing layout, scene, semantics and
  responders. Remove the parallel authored layout tree and object-mapping tables.
- Key causal properties directly by stable typed addresses. Remove static cell
  aliases, prototype-specific key tables, arithmetic ID ranges and renderer-slot
  identity bookkeeping. Renderer IDs remain disposable compilation resources.
- Represent palette entries as admitted definitions and binding/default presets.
  Remove fake palette-only live instances and empty inspector placeholders.
- Use one bounded structural edit batch for creation, movement, removal, rename
  and reconnection. Keep causal property updates independently checked and derive
  all inspector/code entry points from those contracts.
- Replace predecessor hash allowlists, old allocation decoders and compatibility
  branches with fresh-schema initialization. Remove tests that only qualify old
  formats; retain the behavioral race, conflict, reset, bounds, input and restart
  cases against the new format.

Reuse the Merkle/CRDT primitives, shared layout/render/input engines, retained
tooling, host transport and signed deployment machinery. This cleanup does not
select D3 typography/content behaviors, images, XR or general transactions.
The prototype below is local and has not been deployed; its useful behavior
tests and tooling can carry forward without preserving its overlay encoding.

This milestone completes the bounded hierarchy-authoring stage of
[RFC 0035](rfcs/0035-oib-component-designer.md#d2-bounded-hierarchy-authoring).
The delivered [Studio slice](OIB_DESKTOP_STUDIO_D2.md) remains the baseline.
The original repository stays on main; implementation uses its separate worktree.

## Scope and ownership

Admit nested factory insertion, placement into declared slots, owned-child moves
and removal, definition structure editing, and explicit source connections.
Use the existing OIB classes, layout, causal property registers, signed app
handlers and selectors. Neither client-supplied executable/layout bytes nor new
class implementations are an authoring operation. Images, typography, fit-content,
composable D3 behaviors, inherited-child overrides, detachment and document undo
remain outside D2. Portable transfers and the Components conversion remain a
separate milestone; keep the existing transfer refusal until complete UI export
and replacement are implemented and qualified.

Runtime transforms, validation and tests remain in LGPL core. Public records,
signed admission descriptions, app fixtures, tooling and documentation remain
Apache-2.0. Reuse existing dependencies and safe Rust.

## Structural contract

One batch names the observed active document generation and at most 16 typed
operations. The authority checks generation and coherent draft/active state
before allocation. Apply operations to a disposable candidate and validate the
complete closure once before publication. An operation may temporarily leave a
reference unresolved within its batch only if a later explicit repair fixes it.
Rejection persists a bounded diagnostic and the ordinary sequence receipt;
it changes no graph, allocation counter, property register or app value.
Retries require a refreshed generation and an explicit user action. Exact replay
returns the original outcome, including after a lost reply and process restart.

An instance destination is either the admitted document canvas or a declared slot
at a full object occurrence address. An insertion anchor is a stable sibling ID;
absence appends. Ownership, visual parent and order are explicit. Moving a child
preserves its identity, bindings and property registers. Slot-owned objects are
separate from inherited definition nodes. Never reparent inherited content through
an instance selection. Definition edits name the shared definition explicitly.

New authority IDs and new property addresses are allocated monotonically and are
never reused. A fresh duplicate remaps owned objects and their internal references
while retaining definition links and admitted external app bindings. Names, list
rank and renderer slots never supply identity. Failed batches allocate nothing.
Renderer resources are reusable only after the old active graph and local input
release them. Removing a referenced object requires explicit repairs/removals in
the same batch; there is no implicit dangling-reference cleanup or name matching.

Signed class/child/slot rules and admitted factories bound every structural edit.
Source edits select typed literal/forwarded sources or existing admitted app
binding/action pairs. Reconnecting a value also updates its displayed source,
semantics and responder together. A mutable source cannot grant a new action,
field, effect or cross-Space reference. Forwarding cycles and incompatible sources
reject the whole candidate. Ordinary property edits retain independent causality;
a structure batch does not overwrite unrelated property registers.

## Projection, storage and activation

Persist authored structural data and allocation metadata separately from the
immutable signed admission surface. Resolve the current definitions into the
existing layout and property mappings; do not introduce a sample-specific scene
renderer. Retain surviving explicit cell identities through renames and reorders.
Expanded references and slot content count toward all existing limits.

Compile layout, paint resources, semantics and responders as one candidate.
Connected clients wait for confirmed edits and finished gestures/composition
before activation. Removed selections, focus and captures are cancelled against
their old full identities; they never transfer to recycled renderer indices.

Persist state and receipts before acknowledging or publishing. Recovery validates
the signed admission surface and reconstructs the same canonical graph, identities
and outcomes. Keep the existing six live roots, ten resident descriptors, 64 cells,
64 KiB draft/causal budget, 512 scene nodes, 128 controls and 128 KiB layout limit.
Do not raise ceilings to accommodate the fixture. Specify additional structural
source/work limits in the public contract before adding its encoding to state.

The new wire format must bump matching browser/native/server state identifiers.
Release control remains v1 on the shared socket, with the current release hello
on reconnect and code discovery independent of state decoding. Qualify fresh
schema initialization and signed code rollback before live activation; reseeding
affected test data is permitted and no old-data migration is required.

## Acceptance gates

- Real Studio controls insert, move, edit and remove nested owned content; enforce
  slot class/cardinality and protected inherited nodes. Numeric placement precedes
  optional drag tools. Definition edits propagate to linked instances.
- Rename/reorder preserves surviving full addresses and overrides. Referenced-node
  removal rejects without a repair and succeeds with an explicit atomic repair.
- Two clients racing moves yield one acceptance and one durable rejection. A
  deliberate refreshed retry succeeds. Lost-reply replay and authority restart
  preserve the accepted order, property causality, IDs and receipt outcomes.
- Connection changes select only signed sources/actions, retain app values and
  agree with rendered/semantic/control bindings. Invalid cycles, types, references,
  geometry and expansion/resource overflow preserve the exact active graph.
- Focus, composition, held input and delayed retired actions retain or safely
  cancel their original targets through structural activation and resource reuse.
- Focused core and process tests pass, followed by signed ARM/Wasm builds and
  actual Frame/Web authoring acceptance with a compatible code rollback.

Record actual commands, release identities, results and remaining limitations.
Do not claim D2 completion from semantic graph tests or artifact compilation alone.
Do not open a PR until the complete selected scope and live acceptance are ready.

## First increment: shared definition editing

Implemented on the branch; **not yet built for ARM/Wasm or live-qualified**.
The live environment still uses v28. This increment introduces candidate state
protocol **v29**, with matching Rust/browser/publisher declarations and unchanged
release-control v1. It extends the existing Admin-only instance request:

```text
OibInstance { sequence, operation: Definitions(DefinitionBatch) }
DefinitionBatch { generation: u64, operations: [DefinitionEdit] }
```

`otrios_interfaces::oib::authoring` fixes the operation names and typed fields:

| Operation | Fields | Admitted effect |
| --- | --- | --- |
| RenameDefinition | definition, name | Change the shared display name |
| RenameNode | definition, node, name | Change one template node's display name |
| MoveNode | definition, node, owner, visual_parent, before | Move an existing local node using explicit owner and visual parent; preserve identity and parent-local design geometry |
| Expose | definition, property, targets | Replace an existing exposed property's ordered forwarding targets |
| ConnectReference | definition, reference, target, optional source | Set/remove an authored literal or forwarded assignment at an existing reference boundary |

Names are 1–64 NUL-free UTF-8 bytes. Batches have 1–16 operations, a nonzero
generation and at most 8 KiB of encoded source. Paths retain their existing
typed identities and depth limits. Forwarding target arrays are strictly ordered
by full path, with no duplicates. App bindings remain protected in this increment;
literal or forwarding edits cannot disconnect an indirectly bound endpoint.
Node/definition creation and deletion, owned slot content, placement editors and
app binding/action reconnection remain subsequent D2 work.

Encoding uses the existing locked Serde/FlexBuffers serializers and externally
tagged variant names. Accepted structure lives in `[4,8]` as the canonical vector
of definition records, bounded to 16 KiB and included in the existing 64 KiB
resident authored/causal budget. Definition/node identity order is canonical;
visual-child order remains authored. Recovery reencodes the snapshot and requires
byte equality. Its nodes/classes, roots, defaults, reference-definition links,
slot declarations and app binding grants must match the signed admission surface.
Only the mutable fields listed above may differ. Renderer mappings and causal
property registers are not snapshot inputs.

The source hash participates in both active and draft closure hashes under the
`OTriOS\0oib-structure-v1\0` domain. There is one accepted structural snapshot:
an invalid candidate is rejected, rather than retained as an invalid structural
draft. Property conflicts still retain their existing draft/active registers.
Reclamation preserves `[4,8]`; read-only projections include the same active
source while excluding private property drafts. A bounded one-entry compiled-UI
cache is keyed by signed admission identity and structural source hash.

Studio exposes Move shared earlier/later for selected definition children.
The Components tree follows authored visual order and retains actual ownership
separately. These tools use the existing edit/test, dirty-draft, input and Space
gates. Source-aware property/reset behavior uses the current authored definitions.

Initial focused evidence, using Rust/Cargo 1.90.0 from the existing private
builder environment with `--locked --offline`:

- `cargo test -p otrios-core --features canvas --test oib_definition_authoring`:
  11 passed, covering projection, stable cells, literal/forwarded reset sources,
  explicit repair, invalid closure rejection, immutable snapshot admission,
  authority generation races and receipt recovery.
- The focused library test
  `desktop_studio_shared_definition_tools_preserve_instances_values_and_stale_targets`
  passed using actual generated tooling actions.
- A freshly built `dashboard-server`, supplied as `OTRIOS_SPACES_SERVER`, passed
  `spaces_process d1::d2_definition_race_lost_reply_restart_and_readonly_projection_preserve_values`
  (one passed, 8.07 seconds). This crosses a real authority process and exercises
  a move race, lost accepted/rejected replies, kill/restart, exact replay, explicit
  retry, read-only projection and unchanged app/other-app data.
- `cargo clippy -p otrios-core --features canvas,merkle --lib -- -D warnings`
  passed. The shared session/update/text-input Node suites passed all 36 tests.

These are development checks, not completed D2 acceptance. Signed deployment,
compatible v29 rollback and actual Frame/Web authoring checks remain outstanding.
