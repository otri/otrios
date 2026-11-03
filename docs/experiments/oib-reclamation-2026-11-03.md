<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB instance reclamation — 2026-11-03

[RFC 0033](../rfcs/0033-oib-instance-reclamation.md) removes the ten-creation
lifetime budget. Object IDs come from one durable high-water counter. Reusable
renderer slots are independent of object and property identities. After coherent
updates, unused descriptors, causal property/presence leaves and empty branches
are removed from the current shared root. Delayed operations for retired IDs are
acknowledged without affecting a replacement or allocating a duplicate. Current
component registers and ordinary app values remain unchanged.

Restore keeps surviving authored objects, recreates missing factories with fresh
IDs and declared bindings, and reclaims extras. Deleted instance overrides are
discarded; replacements inherit the component design. Signed factory templates
remain in the app package for reuse. Normal bounded snapshot pins and journal
checkpoint rotation have their existing lifetime; this is current-root object
reclamation, not secure erasure of every historical byte.

Wire metadata changes to state protocol v25; release-control v1 stays independent.
The explicit canonical v24 decoder imports the complete old allocation high-water
mark before removing inactive records. The original O1 introduction also remains
supported. No dependencies, services, identities, trust roots or touch calibration
changes are introduced.

Focused checks cover 64 create/remove cycles and bounded checkpoint size, fresh
property IDs on a reused slot, delayed edit/remove/duplicate operations and receipt
retries, pinned snapshots, full-ledger migration, first OIB admission from an older
app checkpoint, shared component layout, Restore/Reset, and held-gesture reuse.
An additional regression repeats pending creation/removal while an unrelated
geometry edit is invalid, proving those unused records are reclaimed without
changing the exact active root or waiting for the other edit to be repaired.
Restoration also validates its replacement before dropping active extras: an
invalid replacement leaves the active roster and allocator unchanged.
The gesture test caught and fixed presentation selectors reading reclaimed
properties before deferred scene activation. The authority process test caught
and fixed replay of first OIB admission onto a checkpoint without OIB metadata.

The read-only pre-upgrade Frame observation found 10 resident historical records,
54 property entries, two active sliders (20/30) and zero remaining lifetime IDs,
at generation 165. Component geometry remained compact with row pitch 80.

## Executed validation

The completed source passed:

- `cargo test -p otrios-core --features canvas --test oib --locked`: all 33 tests,
  including 64 create/remove cycles, both additional draft/Restore guards, held
  gestures and first-admission journal recovery (64.53 seconds).
- `cargo test -p otrios-core --features canvas --lib legacy_full_ledger --locked`:
  the explicit persisted v24 full-ledger migration test passed.
- Strict `cargo clippy -p otrios-core --features canvas --all-targets --locked
  -- -D warnings` passed after the final guards.
- Both real authority-process OIB tests passed with `OTRIOS_SPACES_SERVER` set to
  the newly built server. Both unified session tests passed, including control
  discovery during intentionally unavailable state recovery/obsolete codecs.
- The focused Node session/update/worker tests passed all 34 cases. Python
  release verification passed all 35 cases. Mac tools, Wasm and ARMv7 hosts built;
  complete signed releases and app closures were verified before activation.

No full `scripts/check.sh`, physical finger qualification or broad browser
campaign was run. These are Canvas authority/runtime checks, not Linux
shared-memory IPC acceptance. No cross-release rollback was exercised.

## Deployed and live verified

Final release:
`537317f876a610c8e95b818ef8080bec406d07b94c549079a7231483c0be6bb4`.
The matching verified code recovery kit is under the private builder's
`rollback/2026-11-03-oib-reclamation-code/<release>/`, including the complete signed
closure and matching authority/checker/bootstrap helpers. No app-state backup or
new trust/identity material was created.

The first qualified deployment migrated the existing Frame collection from ten
historical descriptors and 54 property entries to its two active objects and
22 property entries. The next IDs were 65546–65548, above every old allocation.
Three add/remove cycles returned to the original pair and replenished capacity.

The final build repeated that acceptance with IDs 65549–65551; each reused
renderer node 272, and each removal restored two descriptors, 22 properties and
four available slots. A separate invalid track-x draft then retained the active
scene while a new pending object was created and removed. Its unused descriptor
and properties disappeared even while the unrelated geometry error remained.
Repairing the track-x draft restored clean Live status.

The deployed public Wasm client observed generations 172–179, including the
pending object and its removal at unchanged active generation 178. Public
JS/Wasm/catalog/font bytes matched the verified signed candidate. Native PID
18471 and mount epoch 1 remained stable throughout final acceptance. Volume and
Brightness remained 67% and 25%, their original IDs 20/30 remained active, and the
compact 288 × 48 tracks with 80 px row pitch were preserved.

The temporary observer stopped. The existing watcher was restored with current
v25 tooling and source fingerprint
`861e1d3179b1edefb5e7cb6b0323f7ece06faf3f511502a9e35fe3d5a8e570d3`.
Authority, direct display and tunnel were active; the final native view was
confirmed at generation 179. See the [machine-readable acceptance record](data/oib-reclamation-2026-11-03.json).
