<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB library boundary

Date: 2026-11-09. Status: **selected design; library loading not implemented**.
See [RFC 0036](rfcs/0036-upgradable-oib-component-libraries.md) for the normative
selected stages, identity, admission, bounds and acceptance requirements.

The desired iteration loop is to edit a component library, publish a signed
revision and update an app's exact dependency pin. The Frame executable, browser
host module and authority executable remain unchanged for compatible library
updates. A runtime update is needed when adding a new host primitive or ABI.

## What exists and what comes next

| Area | Current D2 branch | Selected next work |
| --- | --- | --- |
| Authored UI | Canonical graph being completed; renderer output is derived | Resolve imported immutable graphs before the same compiler |
| Components | Signed bounded built-in classes and actions | L1 reuses them; L2 supplies generic executable component behavior |
| App executable | Optional `Code/behavior.wasm`, bounded Wasmi app ABI 1 | Keep separate from the new component ABI |
| Packages | Signed app packages and complete release publication | Signed library kind plus exact app dependency locks |
| Baseline changes | Changed signed graph requires fresh collection | Dedicated compatible baseline adoption preserves app data |
| XR | No XR backend/library runtime | Explicit later backend milestone, then library iteration |

The browser's engine runs the outer host WebAssembly; our host embeds Wasmi for
the nested app guest. The native ARM host embeds Wasmi directly. Libraries will
use portable graph/guest bytes, not `.so` plugins. No new WASI, filesystem,
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

L1 adds a library package kind with `Info.flex`, `OIB/library.flex`, integrity and
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

This requires one scoped host/authority update to introduce the loader. Later
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
configuration. L1 adds a separately recoverable library/body pin and compatible
adoption, preserving collection identity, semantic epoch, business data, receipts
and independent property intents. It advances the structural generation and
rejects stale structural work. It must not quietly replace an intent's epoch or
reinterpret an address.

Changing a property type, reusing an ID, removing an addressed node, replacing an
export under the same binding or expanding app grants requires explicit new
admission/replacement. It is not a styling update. No migration of the current
disposable PoC data is required to introduce the new format; routine compatible
updates after that cutover must not depend on collection resets.

## Changes to fix now and changes to defer

For D2 completion, preserve source versus compiler output, stable semantic
addresses, absent-value inheritance, semantic admission/body separation and
strict changed-baseline rejection. Do not add library loader scaffolding to the
current structural-edit cutover.

Before implementing L1, freeze the import-binding codec, distinct library
package/hash/signature envelope, exposed compatibility contracts and recoverable
baseline-adoption record. Connect the loader at
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
ABI/permission/trap/resource handling on both hosts. None of these library-loader
acceptance checks have run; these documents select the work rather than report
it as implemented.
