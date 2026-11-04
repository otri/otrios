<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB component designer D1 — 2026-11-03

[RFC 0035](../rfcs/0035-oib-component-designer.md) D1 delivers an explicit
document/class/property model, stable nested references and signed forwarding,
with a hierarchy, modular inspector and two linked SettingsPanel instances.
The separate [OIB Designer companion](../../apps/oib-designer/README.md) leaves
the existing Live UI schema and factories unchanged. Structure and bindings
remain signed; arbitrary structural editing, rich typography, images and XR are
later stages.

## Accepted release

Source implementation: `59b2707e43e9f8bb07d82fe976e3e34d93557a0b`.
Signed release:
`541cabed97efe0f9df23dd81e6b01b39cd5816b0243d9c00afb29f20ccdd6992`.
ARM authority SHA-256:
`f2367e9ea6c5b5f16d69b94eccbcf474374de71d9ce3a3a869b842728d6da187`.
State protocol is v27; the opening release-control envelope remains v1.

The parent built frozen sources in a separate runtime/Cargo target and Docker
builder, retaining the existing publisher and strict SSH pins. Complete outer
signature/asset verification and inner signed bundle admission passed before
the full deployment. The new app was attached to the existing Experiments Space
through its normal Admin management command. No data was restored, collection
reset, identity provisioned or service added.

Frame authority, native direct renderer, tunnel and monitoring report healthy.
Native signed/runtime hashes match, state is confirmed, input is idle and pending
commands are zero. The existing 90-degree rotation, calibrated touch device and
RGB565 scanout remain in use.

After live acceptance, private release-control promotion verified that Mac and
Frame pointers matched and pinned the resumed Git-backed watcher to
`a52477cc01a4f4e5ec18be53aa4d836ea84273b5`. The source was remotely reachable
before resumption. Recovery code and previous release-control files are retained
in the private runtime independently of this worktree's temporary build outputs.

## Focused live evidence

The public browser loaded the deployed app and rendered the shared Rust/Wasm
scene in landscape and 800×1280 portrait. Nonvisual Configuration selection
showed OIBObject identity with no visual layout properties. A nested Slider
selection showed its complete instance/path, authored/forwarded origins and
read-only signed connection. Hierarchy and inspector scrolling changed their
own visible rows while preserving the preview and shared generation.

An Admin loopback client using the existing native identity changed shared panel
accent to gold: both panel roots and nested slider/toggle accents inherited it.
Audio's nested label override became Sound; Display retained Brightness. The
browser observed revisions 1→2→3 on the same mount epoch, retaining its local
Configuration selection. Causal reset produced revision 4 and restored the
signed authored Volume label. Values stayed 40% and 70%.

Calibrated software evdev input on the real Frame then verified:

- Edit-mode toggle padding selects without changing its value.
- Test-mode button and toggle padding activate the owning whole view.
- Releasing outside the button cancels activation.
- A held Audio slider pins presented revision 4 while the authority confirms
  revision 5; release permits the graph to advance without changing binding or
  value.
- Restoring blue produces revision 6 with authored labels, both original toggle
  values and 40%/70% slider values intact.

Frame kept renderer PID 11051 and mount epoch 3 throughout these data edits.
Hierarchy touch panning left the shared generation unchanged. A subsequent real
authority-service restart recovered revision 6; Frame reconnected with the same
renderer PID, epoch 4 and confirmed state. The browser reconnected on epoch 5,
retaining its nested selection and local scroll positions. A fresh admitted
loopback mount also recovered revision 6. No data rollback was used.

[Bounded acceptance data](data/oib-designer-d1-2026-11-03/acceptance.json) contains
only the public fixture, release identities and selected runtime evidence. The
browser session used guest read-only shared state and local tooling; shared
writes were exercised through the admitted Admin loopback API. This is real
browser rendering and calibrated software input evidence, not a human touch or
latency benchmark. The two initial panels fit the preview; overflow/preflight
and maximum-population culling are covered by the focused runtime tests.

## Validation and review

Actual focused checks used pinned Rust 1.90 and locked offline dependencies:

- `cargo test -p otrios-core --features canvas --test oib_designer_runtime --test oib_designer_fixture --locked --offline`: 17 runtime and nine fixture tests.
- `cargo test -p otrios-core --features canvas --lib oib_designer --locked --offline`: 16 client/viewport tests, including the real six-root fixture, stale IDs, clipping and captures.
- `cargo test -p otrios-core --features canvas --test oib --locked --offline`: 35 legacy OIB regressions.
- Three `spaces_process d1::` cases with an explicitly built dashboard-server: no skips, 98.82 seconds in the parent rerun after the first review fixes. These cross a real authority process with in-process client adapters. The later membership-only admission fix passed the runtime/fixture suites and independent recheck.
- Strict core-library and workspace/all-target Clippy; workspace formatting and diff checks; 15 session and four update JavaScript tests; two FrozenSources builder-isolation tests.
- Mac, browser Wasm and ARMv7 builds, final bounded probe build and signed bundle/release verification.

Independent review found three P2 issues over two passes: removable ownership
dependencies, app binding/rendered-source mismatch, and a factory missing from
the signed document instance list. Parent fixed all three with regressions. A
fresh final review at the accepted implementation reports no findings and adds
two independent closure/converse tests. No production edits were made by reviewers.

Main subsequently advanced with a test-only fixture correction. It merged
cleanly into the handoff branch; 11 bundle tests and five durable app-import
tests pass with `--features canvas,signed-bundles`. These changes are confined
to test code, including a `cfg(test)` helper, and do not alter the accepted
deployed runtime. The watcher retains the exact live-qualified source pin.

The full `scripts/check.sh` campaign was not repeated; focused checks and live
acceptance were used. No Linux shared-memory IPC or separately spawned client
qualification is claimed.

## Compatible code rollback

Complete signed v27 recovery release
`44f01574b84bf784b78f0ce7968ecc5d328572d92940d411f97ab80fa62a4a6a`
and its matching authority, monitoring, bootstrap and bundle-checker binaries
are retained independently of release-cache pruning. Its source is `8e27c4b`;
the signed companion schema/fixture is compatible with the accepted state.
The older v26 live baseline is not the rollback for D1 state. Rollback restores
matching code and signed release control only, never confirmed app data.
Rollback activation was not repeated for this increment.
