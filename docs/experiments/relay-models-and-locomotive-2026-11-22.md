<!-- SPDX-License-Identifier: Apache-2.0 -->
# Shared textured models, locomotive steam and dice contacts — 2026-11-22

`38e8883` processes the supplied website hat from 2,833,832 bytes to 249,264
bytes including its 512 × 512 JPEG texture, 5,277 vertices and 8,989 triangles.
The original silhouette, goggles, band, brim and UV colour detail remain.
Original ambient occlusion is baked into base colour in linear light; unused
normal/ORM maps are omitted. Asset attribution and reproducible processing
commands are in [the asset guide](../../assets/models/README.md).

The recovered Monocle processor is
`monocle3d/scripts/process-progressive-glb.mjs`, used with
`components/gltf/ProgressiveGlbViewer.client.vue` and
`components/hello/AaronExperience.client.vue`. Its 512-pixel AVIF/progressive
candidate was 307,556 bytes before simplifying geometry. The shipped native
subset uses JPEG, avoiding AVIF and meshopt runtime decoder dependencies.

`d7c38ac` gives Steam a small retained locomotive mesh with turning wheels and
connecting rod. Its moving chimney emits independently seeded wisps with
interpolated vertex-alpha feathering. The previous puff loop conditionally
skipped random samples before birth, reshuffling later puffs as they appeared.
Independent per-wisp streams remove that source of flicker. Wisps expand and
rise continuously, with a 6.8-second bounded playback.

Dice now share a 120 Hz bounded simulation with enclosing sphere contacts,
equal-mass impulses, restitution and friction. A bounded separation solve also
protects the interpolation into recorded result slots. Clockwise entry and
recorded face outcomes remain. This is deliberately limited collision physics,
not a general rigid-body engine.

`eec0d6d` adds the reusable resource/instance/camera/frame API documented in
[model rendering](../MODEL_RENDERING.md). It supports bounded static GLB scenes,
multiple meshes/materials, node matrix/TRS hierarchies and shared embedded JPEG
textures. The actual website hat is cached once and receives a 2.1-second forward
tip in a chat-specific adapter. Native GLES2 and WebGL1 consume the same GLSL ES
1.00 source with perspective UVs, directional lighting and depth testing. This
initial material subset is base-colour lighting with a fixed highlight, not
full metallic/roughness PBR. State protocol remains v41; no chat record or app
contract changed. Synchronous bounded graphics FFI invariants are recorded in
RFC 0001. New pinned pure-Rust JPEG dependencies disable SIMD/unsafe backends.

## Focused validation

- `cargo test --locked -p otrios-core --features canvas --lib canvas::chat::effects::tests`: 4 passed; recorded dice faces, clockwise entry, contact impulses/separation through settling, deterministic finite clipped geometry.
- `cargo test --locked -p otrios-core --features canvas --lib static_glb_keeps_uvs_and_shared_textures_and_rejects_unbounded_inputs`: 1 passed (delegate); real asset UVs, transformed/shared instances, malformed ranges, URI, numeric and allocation bounds.
- `cargo test --locked -p otrios-core --features canvas --test canvas brass_relay_particles_do_not_rebuild_the_retained_ui_scene`: 1 passed; locomotive playback reuses retained UI, textured Salute hides behind a modal, returns on dismissal and expires. Initial sample times were moved into the train's visible interval.
- `node --test scripts/test_gpu_scene.mjs`: 19 passed, including texture upload reuse, depth draws, modal suppression and context-loss retirement.
- `python3 -m unittest scripts.test_dashboard_release.SignedReleases.test_precompressed_web_assets_are_deterministic_signed_and_bound_to_source`: 1 passed with the new shader module in the signed closure fixture.
- Changed JavaScript syntax and scoped diff checks passed. No broad preflight suite was run.

Mac tooling, release browser Wasm, ARMv7 release client and Linux authority/checker
builds completed. The manual `otrios-linux-display` `model_pixel_probe` ARMv7
executable ran in the existing builder with `EGL_PLATFORM=surfaceless` and
`LIBGL_ALWAYS_SOFTWARE=1`. Both direct and `OTRIOS_LINEAR_SCENE=1` modes passed
real shader compilation, texture/depth rendering, GL checks and empty-frame
clearing. Rest/tip contained 56,492/76,113 foreground pixels; direct composition
had 5,181/4,835 colours and linear composition 2,031/2,009. Captured rest/tip
pixels were visually inspected. This is software Mesa/native pipeline evidence,
not physical Frame performance or hardware acceptance.

## Signed deployment

Active release:
`af997501a8bcc7541fa978d39bab33eaec5995c32e4ee481173d1a43285a7cec`.
Mac and VM signatures, all asset digests and complete installed bundle closures
verified. All 85 authority files, app contracts and catalog bytes are unchanged.
The v41 Linux authority was rebuilt/restarted to serve the new shader allowlist,
then the verified candidate passed the import guard and became active. Both
release pointers match; compatible baseline
`2ec4e3621bcbb1bd12b0b54f834be8ca11cdeada66f85c08b48e1e9acf727b36`
is retained in the VM's `code-rollback/relay-models-baseline.json` with the
compatible v41 authority available.

Focused live HTTP acceptance confirmed public health/state availability, current
release pinning from `/brass-relay`, and exact signed bytes for model shaders,
GPU renderer, state worker, app, JavaScript bindings and Wasm. Browser UI and
physical iPhone/Frame playback were not exercised during this change. No public
test messages were posted. Acceptance, signature verification and inspected
captures are in `~/Library/Application Support/OTriOSOrbStack/checks/relay-models-*`
and `relay-new-*`. The disposable builder is stopped after finishing imports.
