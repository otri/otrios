<!-- SPDX-License-Identifier: Apache-2.0 -->
# Opening desktop Designer from a phone — 2026-11-22

Designer had no explicit iPhone block, but opening it from a 390 × 720 launcher
failed with `Viewport is smaller than the composition's supported minimum`.
Candidate qualification used Home's current phone viewport. Designer could only
announce its desktop minimum after activation, which that first layout failure
prevented. This was an incoming-layout sizing error, not an admission rule.

The incoming mount now derives its own viewport policy before resolving its first
frame. A uniform scale raises both dimensions to that policy's minimum while
preserving aspect ratio and the existing 8192-unit coordinate bound. Active
presentation and candidate qualification share the same policy calculation.
The retained launcher continues to use its phone density until activation.

Designer can open on iPhone and retains its existing desktop workspace, fitted to
the available aspect. A 390 × 720 host uses a 1280 × 2363 logical workspace;
landscape 720 × 390 uses approximately 1477 × 800. Full phone authoring and a
phone-specific Designer arrangement remain deferred. No UI/admission/data model,
app values, independent library revisions or protocol is changed.

## Focused checks

- Extended the existing `cargo test --locked -p otrios-core --features canvas
  --test spaces_ui launcher_launch_retains_phone_density_until_cached_app_activation`:
  1 passed. The fixture now has a real minimum-width composition. It checks local
  Home sizing during a pending mount, cold/cached destination admission, the
  incoming desktop minimum and host aspect preservation, plus return to Home.
- A real public WSS journey opens the installed signed Designer from its Home
  tile three times. Each cycle fits portrait and landscape, verifies Library,
  Layers, Inspect and Edit/Test controls, retains the inspected app value, and
  returns to a 390-wide Home. No edits are sent.
- Built Mac packer/checker, release browser Wasm, Linux aarch64 authority/checker
  and ARMv7 direct host using pinned Rust 1.90 and the existing dependency lock.
  Broad suites were not run; this is the focused mount/aspect acceptance.

Signed release: `27ea8edf0466114c6e571d8b2fee28199078658cdd21b3546544eb72787f6bb9`.

The complete signed closure is verified before locked atomic activation on the
existing OrbStack/Web host. All 85 authority files, app contracts and catalog bytes
are retained from the prior release. Compatible code and release rollback remain
available. Browser acceptance is recorded after activation below; physical iPhone
Safari interaction and full mobile authoring are not claimed.

After activation, the public WSS journey passed three cycles again, including
portrait/landscape fitting and unchanged inspected state. The actual browser
automatically loaded the signed release, then opened Designer from its Home tile
at 390 × 720. Its URL changed to `/oib-designer`, Library/Layers/Inspect/Edit/Test
controls were present, the Canvas remained 390 × 720 CSS pixels and no application
error appeared. The Home breadcrumb returned to the 390 × 720 launcher; reopening Designer
again succeeded with the same controls and no error.
Authority, tunnel and Monitoring timer remained active. These browser checks use
a desktop engine at phone dimensions; physical iPhone Safari is not emulated.
