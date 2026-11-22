<!-- SPDX-License-Identifier: Apache-2.0 -->
# Relay cinematic effects and modal input return — 2026-11-22

`14ccc13` enlarges procedural Steam to rising viewport-filling clouds, gives
Salute a centered shaded 3D top hat, and makes Boop a truncated-icosahedron soccer
ball entering from the right, shrinking toward the target, bouncing away and
sending perspective ripples toward the horizon. Dice entering from the left now
roll clockwise. Recorded seeds, targets and dice outcomes are unchanged. Effects
span the app viewport, stay input pass-through and hide behind open dialogs.
Their existing 60,000-vertex budget and bounded simultaneous playback remain.

`5455954` fixes the reported `/help` → dismiss → swallowed typing journey. The
shared responder controller previously restored the composer identity while the
Chat editor remained inactive. It now records a one-shot modal-return transition,
and the mutable client resumes the matching editor before presenting or accepting
further input. This is local editor restoration, without replaying activation,
submitting a command, acquiring another draft lease or restarting ordinary
single-line editing after Enter. Browser focus mirroring remains unchanged.

## Focused checks

- `cargo test --locked -p otrios-core --features canvas --lib canvas::chat::effects::tests`: 3 passed, including clockwise entry, recorded face settling, and deterministic finite clipped geometry at phone/Frame sizes.
- `cargo test --locked -p otrios-core --features canvas --test canvas modal_return_restores_text_editing_without_replaying_activation_or_submission`: 1 passed; six `/help` journeys cover Close, back/Escape and native pointer dismissal with external/software keyboard paths, first edit acceptance, no duplicate activation, and intentional single-line Enter termination.
- `cargo test --locked -p otrios-core --features canvas --test canvas brass_relay_particles_do_not_rebuild_the_retained_ui_scene`: 1 passed; effects do not rebuild retained chrome/text.
- `cargo test --locked -p otrios-core --features canvas --lib modal_focus_traverses_restores_and_rejects_rebound_openers`: 1 passed; one-shot nested restoration rejects rebound opener identities.

Actual procedural triangle output was rendered to phone-size PNGs and inspected
for Steam, hat, soccer ball and perspective ripples. This is visual geometry
inspection, not physical iPhone or Frame GPU/FPS acceptance. No dependencies,
public event schema, state protocol or authority contracts changed (v41 remains).

## Signed deployment and live acceptance

Mac tooling, browser release Wasm and ARMv7 release client builds completed.
Signed release `2ec4e3621bcbb1bd12b0b54f834be8ca11cdeada66f85c08b48e1e9acf727b36`
is active on the OrbStack server and Mac publisher. Complete signature/digest
verification and the import guard passed. All 85 authority files, app contracts
and catalog bytes match the prior release. The compatible v41 baseline pointer
is retained as `code-rollback/relay-cinematic-baseline.json` in the VM.

At 390 × 720 CSS pixels the real public browser upgraded automatically, retaining
participant ID, name, avatar and all nine transcript records. The previously
failing `/help` → Close → type `/hel` journey now retains those characters. A
second journey appends `p`, opens help, dismisses with Escape and types `/h`
without clicking the field again; the text remains. Prefixes were cleared and
no test message or live draft was posted. Existing Steam, Salute, Boop and Dice
records replayed locally with no application error, stayed bounded and expired
to zero active animations. Browser capture remains unavailable in this preview
session, so animation appearance was inspected through actual triangle PNGs;
these live checks validate playback state and the shared/browser input journey.
Physical iPhone animation and Frame hardware acceptance remain unrun.

Authority, tunnel and Monitoring timer remain active. The disposable builder is
stopped. Verification and acceptance records are saved in
`~/Library/Application Support/OTriOSOrbStack/checks/relay-cinematic-*.json`.
No broad preflight suite was run.
