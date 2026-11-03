<!-- SPDX-License-Identifier: Apache-2.0 -->
# Live slider geometry and restoration — 2026-11-03

The [component geometry increment](../rfcs/0031-live-oib-component-geometry.md)
adds declared CRDT layout cells to the existing Live UI app. The owner requested
names to the left, a fixed-width track beside each name, percentages at the right,
and tighter stacking. They then requested slightly more spacing.

## Live geometry acceptance

Signed protocol-v23 release
`df731b0ff67af52ff260be7631512ed289719e13ae15940716fbeba3f46145ed`
first admitted the geometry surface with the stacked defaults intact. A single
12-property CRDT batch then produced 128 px label columns, 288 px tracks, a
right-pinned 76 px percentage column, hidden redundant headings and a 64 px row
pitch. It preserved the current Volume **61%** and Brightness **57%** values.

Native presentation advanced **80 → 81** while retaining PID **31126**, mount
epoch **1**, the same signed code release, Brightness inspection and inspector
offset **232**. The deployed public JS/Wasm/catalog/font hashes matched the
verified candidate. That actual Wasm client observed the change with its own
Volume inspector selected and offset **100**, also retaining epoch 1.

The follow-up spacing edit changed only cell 112 to **80 px**, adding 16 px between
rows. Native presentation advanced **81 → 82** with the same process, mount and
release. Its tracks remained 288×48 px. The owner's intervening slider drags had
changed the displayed values to **53%** and **48%**; the spacing edit preserved
those values too. The owner explicitly accepted both the compact design and the
80 px spacing. [Geometry evidence](data/live-slider-layout-2026-11-03/geometry.json)
records the data batches, scene geometry and independent local client state.

The owner subsequently exercised removal until all original sliders were gone,
then added another instance. Add uses the first signed factory (Volume), so it
could not recreate the deleted Brightness binding. No state or history was erased
to work around that limitation. The [restoration increment](../rfcs/0032-oib-restoration.md)
adds explicit recovery controls.

## Validation

The geometry increment passed 24 OIB tests, 69 Canvas integration tests, two real
authority-process OIB authorization/recovery tests, two unified-session tests,
34 Node session/update/worker tests and 35 Python signed-release tests. Strict
Clippy passed for the five changed packages and all targets. These checks include
exact predecessor admission/journal replay, invalid geometry retaining the active
root, slider endpoints after resizing, late-created instances, and held-gesture
activation without remounting. Signed Mac/Wasm/ARMv7 builds and complete outer
release/inner bundle verification passed.

The restoration follow-up adds tests for recovery at the lifetime allocation cap,
reactivating original identities and bindings, keeping app values, Member rejection,
idempotent retries, invalid-draft recovery and unknown-outcome reset across real
authority restarts. Its focused OIB target passes **26 tests**, the two process
tests and unified-session target pass, and the v24 Node/Python release checks pass.
Strict package/all-target Clippy and a final core-library check passed. The full
`scripts/check.sh` suite was not rerun. No Linux shared-memory IPC claim is made.

No dependencies, services, identities, trust roots, kernel, touch firmware or
calibration changes were introduced. Compatible signed code-only recovery kits
are kept privately; they contain no app data or credentials. A separate cross-release
rollback was not exercised. Runtime implementation/tests remain LGPL-3.0-only;
contracts, app authoring, SDK/host glue and tools retain Apache-2.0.

## Restoration deployment

Signed protocol-v24 release
`e42845712c5dadc91f515af29361faac886c3d483482eb6c0a1afc5927bda35d`
is active on Frame/Web. Native was at generation **97** with only an added Volume
instance active; original IDs 20 and 30 were inactive. A calibrated synthetic tap
on **Restore sliders** advanced to **98**, reactivated those original objects and
their Volume/Brightness bindings, deactivated the extra instance, and retained
the compact 80 px design. It consumed no allocation slots.

A synthetic tap on **Reset UI defaults** advanced to **99**, restored the signed
216 px stacked layout and cleared UI overrides. Both displayed slider values
were unchanged. The approved compact values were then reapplied in a single CRDT
batch, advancing to **100** with two 288×48 px tracks at an **80 px** pitch. The
same native PID **1605**, mount epoch **1** and signed release covered all three
operations. Original IDs 20/30 are active; the seven-entry ledger and three
remaining lifetime allocation slots are retained.

The actual deployed public Wasm client observed generations 97, 98, 99 and 100.
Tapping the two inline names through calibrated evdev selected Brightness and
Volume locally without changing generation 100. The existing authority, native
direct host and tunnel were healthy. The final state leaves the authored pair
with the owner's accepted compact geometry and 80 px spacing, preserving app
values. [Restoration evidence](data/live-slider-layout-2026-11-03/restoration.json)
contains the UI-button coordinates, graph identities, value comparison, geometry,
native readiness and second-client observation. This is device software acceptance;
the owner had already gone to bed, so no human acceptance of the new buttons is
claimed. Their prior compact/spacing acceptance remains recorded above.

The final v24 code-only kit was verified before activation and remains available.
The development watcher was restored to the accepted source/release. No broad
preflight or cross-release rollback was repeated for this follow-up.
