<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB library boundary

Date: 2026-11-09. Status: **L1 implemented and focused live-qualified; L2 and XR remain proposed**.
See [RFC 0036](rfcs/0036-upgradable-oib-component-libraries.md) for the normative
stages, identity, admission, bounds and acceptance requirements.
[L1 qualification](OIB_LIBRARY_L1.md) records the deployed result.

The desired iteration loop is to edit a component library, publish a signed
revision and update an app's exact dependency pin. The Frame executable, browser
host module and authority executable remain unchanged for compatible library
updates. A runtime update is needed when adding a new host primitive or ABI.

## What exists and what comes next

| Area | Current runtime | Later work |
| --- | --- | --- |
| Authored UI | Canonical local graph plus verified immutable imports; renderer output is derived | Richer separately selected controls/content |
| Components | L1 reuses signed bounded host classes/actions | L2 supplies generic executable component behavior |
| App executable | Optional `Code/behavior.wasm`, bounded Wasmi app ABI 1 | Keep separate from the new component ABI |
| Packages | Independently signed library kind, exact app locks and complete release closure | New package/ABI requirements need explicit admission |
| Baseline changes | Compatible library adoption preserves app data/overrides; other signed-source changes require fresh admission | Future additive import/replacement operations |
| XR | No XR backend/library runtime | Explicit later backend milestone, then library iteration |

The browser's engine runs the outer host WebAssembly; our host embeds Wasmi for
the nested app guest. The native ARM host embeds Wasmi directly. L1 libraries use portable graph bytes; L2 will add component guest bytes.
Neither uses `.so` plugins. No new WASI, filesystem,
network, unsafe runtime code or trust roots are selected here.

## Responsibilities

The runtime owns rendering/layout primitives, clipping and hit testing, input
capture/bubbling/modal barriers, state validation, causal edits, immutable
publication, signature verification and bounded sandbox execution. These are
stable mechanisms, not a catalog of every product widget.

The library owns reusable composition, styles/defaults, exposed typed
properties/slots/outlets, and later widget behavior over those primitives. L1 is
data-only: it cannot invent a new input, font/image or rendering primitive. L2
adds a generic typed event/context/command dispatcher once, then allows new
widget behavior without another host enum variant per widget.

The app owns business data, explicit library pins, local definitions/wrappers,
instance assignments and admitted connections to state. Importing a library
does not give it the app's grants. Imported definitions are immutable inside an
app; author a library revision or make an explicit new local component.

## The first loader milestone

L1 provides a library package kind with `Info.flex`, `OIB/library.flex`, integrity and
notices, using existing signed-package rules. An app pins up to four exact direct
library revisions. There are no floating versions, transitive dependency solver,
remote resources or new guest behavior in this milestone.

The load path is:

1. Publish complete verified library packages and an app manifest lock through
   the existing signed release workflow.
2. Resolve stable export/import identities into a transient canonical graph.
3. Validate compatibility, local edits/connections and all aggregate limits;
   compile the complete candidate off the active path.
4. Wait for confirmed edits and finished gestures/composition, then adopt the
   lock and scene coherently with the expected structural generation.
5. Recover that exact pin after restart; rollback through the same validation.

The deployed host/authority update introduced the loader. Later
compatible library-body updates change package bytes and pins. Multiple apps
can upgrade independently. A stale or invalid candidate leaves the prior scene,
pin and data active. Discovery remains available through release-control v1
even if a client cannot decode state or a new library format.

## Identity and default inheritance

Separate logical `LibraryId` and `ExportId` from immutable revision hashes.
Persist a stable mapping to document-local `DefinitionId`s; retain node/property
IDs and complete occurrence ancestry across compatible revisions. Names,
renderer indices and content hashes never supply semantic identity.

Keep immutable library defaults outside local structural records and causal
registers. An absent register means inherit the current authored/library default.
Do not initialize an untouched property by writing its old default as an
override. A library style change then updates untouched instances while explicit
shared/instance overrides retain their values and causal contexts. Reset reveals
the currently pinned default through the ordinary observed-reset semantics.

The D2 boundary cleanup separates semantic admission from graph body bytes. Its
selected epoch binds incarnation, scope, UI reference and logical graph ID with
the schema/class/property/grant contract. Full source equality still gates
configuration. L1 provides a separately recoverable library/body pin and compatible
adoption, preserving collection identity, semantic epoch, business data, receipts
and independent property intents. It advances the structural generation and
rejects stale structural work. It must not quietly replace an intent's epoch or
reinterpret an address.

Changing a property type, reusing an ID, removing an addressed node, replacing an
export under the same binding or expanding app grants requires explicit new
admission/replacement. It is not a styling update. No migration of the current
disposable PoC data is required to introduce the new format; routine compatible
updates after that cutover must not depend on collection resets.

## Foundation and remaining work

D2 now separates source from compiler output and preserves stable semantic
addresses, absent-value inheritance, semantic admission/body separation and
strict changed-baseline rejection. It contains no library loader scaffolding.

L1 fixes the import-binding codec, distinct library package/hash/signature
envelope, exposed compatibility contracts and recoverable baseline-adoption
record. The loader connects at
bundle admission, the OIB graph compiler, `configure_oib`/state binding and client
mount/release activation. Keep expected-generation baseline adoption separate
from structural user edits and general transactions.

For L2, freeze canonical typed event/result records and memory-buffer ABI vectors,
then qualify fuel/memory/store limits on Frame and Web. Declare guest local
properties and typed outlets; the host validates complete command lists and maps
outlets only to app-admitted state actions. Guest memory is scratch; confirmed
host-owned properties survive a compatible guest update at a safe boundary.
The old five-lane app ABI and D2's single-source actions are insufficient for this.

XR remains later. New tracking/display/spatial input and scene primitives need a
host/backend change. Once admitted, XR component graphs and behavior can follow
the same package and upgrade model.

## Evidence required before claiming completion

Show a signed style/layout update on the actual Frame and Web with unchanged
host binary/module hashes and preserved collection, values and overrides. Check
restart recovery, compatible rollback, a held gesture, stale-generation
rejection, bad package/API/limit rejection and bounded slow-reader resync. L2
also needs a changed widget implementation without a host rebuild and qualified
ABI/permission/trap/resource handling on both hosts. The [L1 record](OIB_LIBRARY_L1.md) includes these bounded
data-library results.
Component behavior ABI L2 and XR acceptance remain future work.
