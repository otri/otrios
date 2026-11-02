<!-- SPDX-License-Identifier: Apache-2.0 -->
# Live OIB instances — 2026-11-03

O2 implements [RFC 0030](../rfcs/0030-live-oib-instances.md): named instances
created, duplicated and removed through UI controls or the same typed Client
operations. The [usage guide](../OIB.md) describes ownership, inheritance, APIs
and retained history limits. Executable main-class/outlet APIs remain deferred.

## First signed deployment

The first signed O2 release was
`86cc5fb9d17a7d39673c6e0bd37ce05884449d2c8394ee42d45b5cb69d57b3c8`,
with state protocol **v22** and the unchanged release-control **v1** envelope.
The existing authority, native direct host and tunnel were healthy after
activation. The current Live UI attachment migrated its exact admitted O1 seed
to O2, retaining the original properties and app-content fields. Active generation
advanced from 66 to 67 for the seed upgrade.

On Frame, a guarded synthetic calibrated evdev tap on **Add LabeledSlider**
created `instance-3`, ID 65538. Confirmed native presentation advanced to generation
68 in 1.208 seconds, including the held contact and readiness polling. Native
PID **24553** and mount epoch **1** stayed unchanged during the data edits.
This is device software acceptance, not a human touch or latency benchmark.

The bounded ARM [qualification client](../../crates/otrios-dashboard/examples/oib_instance_probe.rs)
used Frame's existing native identity and the normal loopback session. It issued
the public Client methods to create `code-slider`, set its label override to
`Code`, change the component label to `Shared`, duplicate to `code-copy`, and
remove both temporary code-created instances. The duplicate retained its copied
override while the other instances inherited `Shared`. IDs 65539 and 65540 remain
allocated and inactive; their identities are not reused. An attempted null
component assignment was rejected locally: only instance overrides can be
cleared. A following explicit component default restored `Level`, and a named
instance-property edit set `instance-3`'s label to `Frame` at generation 76.

The observer fetched the deployed public JS, Wasm, catalog and font over HTTPS,
checked their SHA-256 identities against the verified signed candidate, and
instantiated that actual Wasm host in Node. Its read-only public WebSocket mounted
the same attachment. It observed every active generation **67–76**, the copied
override and both removals. Its mount epoch remained **1**, selected inspector
remained **brightness (30)** and inspector offset remained **388** throughout.
Meanwhile Frame locally inspected instance 65538 at offset 344. Local inspection
did not publish an OIB generation. This is live browser-codec/client acceptance;
it does not claim browser GPU or DOM interaction coverage. The user separately
confirmed that scrolling the preview on the web worked.

## Frame preview touch-pan follow-up

The user found that the left preview did not appear scrollable on Frame. Wheel
scrolling worked, but a touch beginning on empty space hit the decorative card
behind the viewport. The preview router required the hit path to contain the
viewport, so it rejected that background before the scroll controller could
capture it. Text painted inside the viewport could behave differently.

The correction admits noninteractive background hits through the viewport's
containing card. The scroll controller still checks viewport bounds. Slider
capture, other panels and modal/Consume barriers retain their input. A regression
uses a background point whose hit path excludes the viewport, pans through actual
pointer samples and verifies a changed local preview offset with unchanged
inspector selection, inspector offset and shared root. It failed before the fix
and passes afterward.

The signed touch-pan follow-up is
`6fc39d970dcec7e1dc9471ae92907c52b96c43f804433af3069034404f6757f5`.
Focused calibrated evdev acceptance panned the preview from **0 to 184**, selected
the third instance, tapped **Duplicate**, panned to **400**, selected the copy and
tapped **Remove**. Removing the copy clamped the preview back to 184; a downward
pan returned it to 0. The two shared button operations advanced generations
**76 → 77 → 78**, retaining native PID **24997** and mount epoch **1** throughout.
The three original active instances remained, with the third locally inspected
and the independent inspector offset at 344. Four lifetime allocation slots
remain after this acceptance. The user then explicitly confirmed that physical
touch dragging scrolls the left column.

The final public JS/Wasm/catalog/font identities were checked again, and the
actual final Wasm client mounted generation 78 with all three active instances.
The existing development watcher was restored with the accepted source and
release. [Bounded acceptance evidence](data/live-oib-instances-2026-11-03/acceptance.json)
records both releases, native gestures, per-client local state and final service
readiness; it contains no authentication material or private app content.

## Validation and recovery

Focused validation passed using pinned Rust 1.90.0 and locked dependencies:

- Core OIB integration target: **20 tests**, including exact signed O1 migration
  and journal recovery, authorization, conflict retention, removal versus
  concurrent property edits, lifetime allocation exhaustion, stable responders,
  held-gesture activation and reconnect with local inspector state.
- Core Canvas integration target: **69 tests**.
- Two real authority-process OIB tests: Member/forged-write rejection, durable
  unknown-outcome retry, restart recovery, retained removal and fresh identities.
- Unified-session integration target: **2 tests**, including release discovery
  despite incompatible state/recovery failure.
- Node session/update/worker tests: **34 tests**.
- Python signed-release tests: **35 tests**. Socket-dependent checks passed on the
  permitted rerun; sandbox bind failures were not treated as runtime failures.
- Strict Clippy for the five changed packages/all targets, plus the authenticated
  diagnostic helper. OIB tests were rerun after the touch-pan correction.
- Mac, browser Wasm and ARMv7 builds; complete signed outer release and inner app
  bundle verification before activation.

No new dependencies, services, identities, trust roots, kernel or driver changes
were introduced. Frame's accepted touch mapping and udev calibration remain in
place. The private code-only recovery kit retains the complete verified O2/v22
release and matching host/verifier helpers independently of cache pruning. It has
no app data or credentials. A v21 binary is not a compatible rollback after this
state upgrade. No cross-release rollback or full `scripts/check.sh` run was
repeated for this increment; no Linux shared-memory IPC claim is made.

The first delivery admits **six live instances and ten lifetime allocations**,
including the seeds. Removed identities remain retained. New sliders use the
signed Volume binding; duplicates retain their source binding. Independent new
app fields, arbitrary placement/reparenting, mutable bindings, history collection,
general seed rebasing and portable OIB transfers remain outside O2.

## Panel height follow-up

The user requested that the left and right panels extend to the bottom of Frame's
viewport. Their wide arrangement now gives the row and both component slots a
flexible height. The tall arrangement shares the remaining height between its
stacked panels. Existing internal top/bottom pins resize the clipped content
viewports. At 1280×800, both cards end at **y=776**, retaining the **24 px** outer
bottom margin; their heights grow from 500 to 656. The preview viewport grows
from 464 to **620**, and the inspector viewport from 364 to **520**. At 800×1280,
the stacked arrangement ends at y=1256 with the same outer margin.

Panel layout constraints are still authored in the signed app presentation.
This adjustment is a signed app update, not a CRDT edit to panel geometry. The
live-editable surface remains declared component properties and instance
operations. Making all layout constraints editable requires a later OIB surface.

Signed presentation updates may now retain the **exact canonical OIB seed**
without reinitializing the instance ledger. The UI binding/hash and status
generation advance while draft/active properties, presence, immutable allocations,
removed identities and app-content values survive. Modified seed definitions
still require an explicit compatible migration; the existing exact O1 migration
also remains supported. The wire format and protocol remain v22.

The focused OIB target passed **21 tests**, including the previous deployed O2
fixture, an active instance override and removed allocation surviving the new
presentation, journal replay, unchanged active root, and actual wide/tall panel
bounds. Strict core-library Clippy also passed. The prior signed O2 UI fixture is
Apache-2.0 data; its migration tests and runtime remain LGPL-3.0-only. No new
dependencies were added.

Signed release
`ececaf3d4dd97ccc13d92e7eaad2e60db3163ba203b2f7c13df872dc8e8575bf`
is active on Frame/Web. Native generation **79** retains all three active
instances, three removed allocations, their overrides and four remaining lifetime
slots. Calibrated synthetic touch pans independently reached preview offset 28
and inspector offset 232, then returned both to 0. All pans retained native PID
**26315**, mount epoch **1** and generation 79. These limits confirm viewport
heights 620 and 520 with the new panel bounds. The actual deployed public Wasm
client mounted generation 79 with the same limits and retained `Frame` override.
The signed artifacts, native state stream and existing three services were
verified healthy. [Panel evidence](data/live-oib-instances-2026-11-03/panels.json)
records the results without pixels or authentication material.

A compatible signed code-only kit for this presentation-binding behavior was
verified and retained before activation. Use that kit rather than an older
authority that rejects the changed UI binding. The development watcher was
restored to the accepted source/release. No cross-release rollback or broad
preflight suite was repeated for this follow-up.
