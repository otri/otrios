<!-- SPDX-License-Identifier: Apache-2.0 -->
# Touch Tracing, developer preferences and additive install — 2026-11-23

Touch Tracing is installed at `test-playground/touch-tracing` in the existing
Public Space. It uses app-authored grid/border/toolbar UI and reusable local
`InkOptions`, `Ink` and `ClearInk` operations. There is no app-name branch in the
runtime. Strokes are RAM-only, retained until Clear/unmount, and bounded to 64
segments of at most 192 points. No drawing files or shared stroke stream are
created. The Components pad retains its existing five-second shared trails.

Signed runtime release:
`de6cce4e419632bac136612dd2a29456a60451c19b9ee85ef54ddf47b958cfc7`.
Signed app-addition release:
`61e108276411792f3326143c1daff0bc1bb4b8392e49d7d5e3079c488b5849e8`.
Touch Tracing bundle digest:
`8ef7ac32cf74457e6552681665aafdc0b8ca25b3b7ad47ce080269f30d5752a0`.
Native binary SHA-256:
`5e3e78949587c5ec5a377265ab8f241690710272a6e0237431ff1c3aff83cf48`.

The first release upgrades the shared native/Web runtime and state protocol to
v43, preserving release-control v1. It keeps every existing app bundle and
contract unchanged. The second adds only Touch Tracing and its catalog entry;
all host assets, old bundles and old contracts remain byte-identical. Complete
signed closure verification passed before activation. The compatible v43 runtime
release and authority binary are retained as the development rollback.

## Live acceptance

The existing Frame display supervisor was updated during the runtime deployment.
The subsequent app addition was adopted without restarting the native process:
PID **16587** before and after, with `runtime_release` staying on the runtime
release and `ui_release` advancing to the app-addition release. The app was then
attached to Public with the existing offline provisioner and existing Admin;
no identity, membership, credential or trust root was added. The brief authority
restart also left Frame's native PID unchanged.

Using calibrated evdev injection against the real Frame UI:

- Opened Test Playground and Touch Tracing through their visible launchers.
- Injected a 305-sample perimeter/diagonal gesture; the bounded local renderer
  retained **302 points in two segments**. No replicated commands were pending.
- Waited six more seconds: the stroke statistics stayed unchanged.
- Clear reduced points to zero; Exit returned to Test Playground.
- Reopening started empty. Frame was left on the empty drawing grid, PID 16587.

The initial coordinate probe used browser profile controls; native omits Sign out,
which moves Close. Its `--native` option now reproduces the actual Frame panel.
An accidentally opened profile editor was cancelled without a name change before
repeating the launcher journey successfully.

Developer Mode and Hit Zone on Touch defaulted off. The authenticated, non-Admin
Frame profile could enable Developer Mode, reveal its theme/hit-zone controls,
save those preferences, and retain them across the authority restart. The final
preferences were restored to System appearance, Developer Mode off and Hit Zone
on Touch off. Keyboard focus/responder behavior is independent of this overlay.
Monitoring's local timer and publisher remained active; the display had connected
Monitoring using its existing identity before the drawing journey.

Physical tracing accuracy remains for the user to assess on return. No new touch
calibration was installed. No interactive browser qualification or broad suite was
run for this slice.

## Focused validation

Passed during this work:

- `cargo test --locked -p otrios-core --features canvas persistent_ink_retains --lib`
- `cargo test --locked -p otrios-core --features canvas --test canvas touch_tracing_is_fullscreen_local_persistent_and_clearable`
- Existing self-profile persistence/revision-conflict regression extended with preferences.
- `cargo test --locked -p otrios-core --features canvas developer_controls_are_self_profile_only_and_hidden_until_enabled --lib`
- `node --test scripts/test-dashboard-session.mjs` — 15 passed.
- Focused Python and JS additive signed-app classification regressions.
- `bash scripts/build-dashboard-web.sh` through the pinned build environment;
  ARMv7 direct host/collector/bootstrap release builds in the existing builder.
- OrbStack authority, bundle checker and native coordinate probe builds.
- Signed installer/activation, exact host-byte comparison and native binary hash check.

Machine-readable release, profile and drawing evidence is retained in the existing
Dashboard runtime's `build/touch-tracing-20261123/` directory. The signed releases
remain in its immutable release store and the OrbStack authority store.
