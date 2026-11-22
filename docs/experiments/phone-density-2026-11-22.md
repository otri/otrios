<!-- SPDX-License-Identifier: Apache-2.0 -->
# Browser interface density — 2026-11-22

The previous shared shell imposed a 1280 × 800 minimum on Home and every app.
Fitting that minimum into an iPhone made text and touch targets shrink with the
phone width. Chat separately selected fixed logical widths (534, 800 or 1280),
so its horizontal scale also changed across device widths and rotation.

Home, collections, chat and Monitoring's shell now declare a 320 × 300 minimum.
At ordinary phone/desktop browser dimensions, their responsive scene uses one
logical unit per CSS pixel. Device-pixel ratio continues to determine backing
resolution only. Chat uses the actual CSS width through keyboard resizing and
rotation instead of a fixed-width density bucket. Below the supported minimum,
the minimum still applies; this is not a physical millimetre calibration.

Narrow headers use a compact heading role and a second app-location row. Launcher
cards reflow and retain scrolling. Toolbars and map input bounds move below that
row together. Chat's room controls fit beside each other; its composer remains
within the keyboard-shortened viewport. At very short heights, remote typing
previews yield space to the composer. Browser pinch magnification is enabled
while one-finger Canvas gestures retain their existing ownership.

Monitoring keeps its fitted 16:10 chart content under the independently readable
shell. Designer and experiment content retain their desktop/content minima.
This focused pass does not qualify full phone authoring or all demo content.

## Validation

- `node --test scripts/test-viewport.mjs`: 7 passed. Covers CSS-pixel density at
  widths 320, 375, 390 and 430, landscape, keyboard heights, and consistent pointer
  and semantic coordinate mapping. Existing signed content-minimum cases pass.
- `cargo test --locked -p otrios-core --features canvas --test spaces_ui
  launcher_collections_preserve_admission_management_and_parent_navigation`:
  1 passed. Extended the existing regression to check 320/390/430-wide collection
  headers and touch-sized ancestor, account and arrangement controls; the existing
  desktop/admission/navigation checks still pass.
- Built Mac packer/checker, release browser Wasm and ARMv7 direct host using the
  pinned Rust 1.90 toolchain and existing dependency lockfile. Built the Linux
  aarch64 authority/checker and temporary read-only acceptance probe.
- A real public WSS journey mounted Home, Brass Relay and Test Playground at
  390 × 844 with matching controls in Light/Dark and functioning account menus.
  Chat navigation, room actions, composer and Send fit at 320 × 568, 430 × 932,
  844 × 390, 390 × 420 and 390 × 300. No edits were sent. Monitoring's signed UI
  and existing private feed rendered locally with a 342 × 213.75 fitted chart
  beneath the 390-wide header; this is not an authenticated Monitoring login test.

Active signed release:
`01530a5d74252653ca5b7b24317f0a61ffdca5e4113fa134e22fab33c498a60b`.

The complete signed closure was verified before locked atomic activation on the
existing OrbStack/Web host. All 85 authority files, app contracts and catalog bytes
are unchanged from the shared-shell baseline. State protocol v40 and opening
control v1 are unchanged. After activation, the public WSS journey passed again;
13 canonical HTTP routes returned 200, the removed aliases remained 404 and
anonymous `/auth/session` remained 401. The public release pointer and downloaded
viewport module, CSS and Wasm matched the verified release manifest.

The prior `cb2f5c813921d6c4a89edac34b2c7c6cde899449b80b346d88b1bbaa7976db67`
release and compatible authority executable remain available for rollback.
Broad preflight suites, physical iPhone Safari interaction/pixel acceptance and
physical Frame acceptance were not run. The phone result above is shared-scene,
viewport-math and protocol acceptance. Frame remains offline. No remote push or
PR was made.

## Follow-up: repeated Relay entry

The iPhone report exposed a missing transition case in the initial acceptance:
that check mounted chat directly; it did not repeat a launcher tile activation
with browser viewport-policy feedback between every mount message.

A real public WSS reproduction activated The Brass Relay from Home while applying
the browser's responsive sizing calculation. Home remained visible during the
mount handshake, but `host_active` had already been cleared. Its reported policy
therefore changed from the phone minimum to the desktop minimum, expanding a
390 × 720 scene to 1280 × 2363. The pending transcript then failed with
`Document viewport exceeds GPU geometry budget at this scale`. The web owner
reconnected and returned to Home, matching the reported small/normal Home loop.

The policy now follows the still-visible local launcher until the destination
actually replaces it. It retains the phone minimum throughout mounting, regardless
of whether input ownership has transferred. App state, cache contents, transcript
limits and wire protocols are unchanged.

- Added and ran `cargo test --locked -p otrios-core --features canvas --test spaces_ui
  launcher_launch_retains_phone_density_until_cached_app_activation`: 1 passed.
  Checks pending launch policy, metadata arriving during mounting, a full UI mount,
  a cached UI mount, and return to Home after activation.
- The public WSS shared-client journey now completes three consecutive
  Home → Relay → Home cycles with viewport feedback throughout, including two
  cached UI hits, a steady 390 × 720 scene and no edits sent.
- Rebuilt browser Wasm, Mac packer/checker, Linux authority/checker and ARMv7 direct
  host using the existing pinned toolchain. No broad preflight suite was run.

Follow-up signed release: `560c46f2b53253049af423f1fc86671bb12489e899fe331b366728b66d22a5cf`.
The complete closure was verified before atomic activation. All 85 authority
files, app contracts and catalog bytes remain unchanged. After activation, the
three-cycle public WSS journey passed again with two cached UI hits and no scale
change. Home/Relay HTTP returned 200; the public pointer and downloaded Wasm digest
matched the signed release. Authority, tunnel and Monitoring timer remained active.
The compatible prior release and authority executable remain available for rollback.
Physical iPhone Safari acceptance remains with the reporting device.
