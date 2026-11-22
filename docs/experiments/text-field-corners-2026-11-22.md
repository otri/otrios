<!-- SPDX-License-Identifier: Apache-2.0 -->
# Matching text-field borders — 2026-11-22

The iPhone New Space field exposed two causes: Home nested two already-outlined
surface helpers, producing four paint layers, and the shared squircle paint did
not match the native HTML editor's circular corners.

The Home field now has one outline/fill pair with 12/10.5-unit radii and a
1.5-unit inset. Shared generic editor presentation gives its background circular
corners. This covers New Space, rename fields, display name, person lookup,
shared app URL and hex color, plus Text Input playground fields. Designer property
and layout fields likewise use circular 12-unit outer corners and a matching
inset, while unrelated buttons retain their existing styling. Relay inputs
already use matching circular curves. Native sign-in fields have a single CSS
border and no underlying Canvas field.

The audit also found that the previous focus change excluded Designer's local
`KeyboardKey(u16::MAX)` control sentinel as though it were a software key.
Commit `55676eb` keeps those controls eligible for shared focus while actual
software keys continue delivering to the text first responder. The existing
Designer editing test failed before this correction and passes afterward, with
an explicit first-responder assertion. Commit `876a3af` contains the border fix.

Focused validation:

- `cargo test --locked -p otrios-core --features canvas --lib frame_focus_direct_inputs_commit_once_and_reset_without_changing_bindings`: 1 passed, 592 filtered.
- `cargo test --locked -p otrios-core --features canvas --test spaces_ui local_forms_use_the_shared_keyboard_without_synchronizing_drafts_as_app_data`: 1 passed, 46 filtered.
- Mac publisher/tooling, browser Wasm, Linux authority/checker and ARMv7 release host builds passed. No broad preflight suite was run.

Signed release `07fc4bc02bd444f893324e0f0387c9ff2dde1a0c8926040fb4bd19aaac3fe12f`
passed complete signature/digest verification on both hosts and is active in
both publisher and OrbStack pointers. All 85 authority files, app contracts and
catalog bytes remain unchanged. The VM retains signed baseline `d9264ac450ef2cb0e2baa318640e8ba4a21768c55edc37c169e94b8340a9bb54`
and its compatible authority executable for development rollback.

The public browser automatically upgraded. Selecting a Designer object exposed
its Width editor with a computed 12px corner and 1.5px inset border. Focusing it
selected the same node in shared focus state; Tab moved to Done with visible
keyboard focus and no application error. No property value was changed or
published. The admin-only New Space flow is covered by the shared fixture;
physical iPhone corner pixels were not retested.

After the prior focus work was accepted on iPhone, the builder maintenance dry
run and apply completed with no blocked entries. No automatic watcher was
configured/running, and the disposable Docker builder is stopped. Mac pruning
retains the current release; VM rollback remains available. Authority, tunnel
and Monitoring timer remain active.
