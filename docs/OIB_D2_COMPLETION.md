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
binds the collection incarnation, signed OIB source and admitted schema/selector/action grants; it survives a restart of
the same collection and changes on fresh initialization. The authority checks it
before allocation or receipt replay. Clients capture that epoch when queuing an
intent; restoring RAM intents against a different mounted document discards them
instead of substituting the new epoch. An ordinary presentation update with the
identical signed OIB source retains the epoch. Changing that signed source or those grants requires
a fresh collection; predecessor-specific rebasing is gone.

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
shared definitions, durable instance descriptors, the monotonic allocation cursor,
placements and typed app connections. The old independent allocation/cursor,
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

The definition-lowering cache uses the signed admission identity and definition
content hash, so roster or placement changes do not invalidate it unnecessarily.
The production Designer fixture no longer emits empty per-preset inspector nodes.
Legacy allocation decoding, predecessor allowlists and migration-only tests were
removed. The retained wide-track fixture uses the current format and qualifies
input/layout behavior rather than an obsolete migration.

## Remaining structural cutover

This foundation is **not the complete graph compiler or D2 acceptance**. The signed
admission surface still carries the flat application layout, property-output
aliases and object/property lowering tables. Remove those by generating them from
the canonical definition graph. Replace palette-only factory objects with explicit
presets, then consolidate the older lifecycle requests and definition/hierarchy
batch variants into the single editing contract. Do not retain two permanent
runtime modes to preserve old PoC data.

The checkpointed authoring prototype already exercises shared rename/reorder,
exposed forwarding, reference assignments, owned slot insertion/movement/removal,
subtree duplication, placement and admitted app binding/effect reconnection. Carry
those behaviors and their tests forward to the graph compiler. Definition/node
creation and deletion and the complete corresponding editor surface remain work.
The production signed fixture must declare useful slots before live slot authoring
can be accepted; test-only slot declarations are insufficient.

## Validation and completion gates

Use Rust 1.90.0 and the committed lockfile. Runtime code/tests remain LGPL core;
contracts, app descriptions, tooling and docs remain Apache-2.0. No dependency or
unsafe-code addition is needed for this cleanup.

Focused development evidence for the format foundation:

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
