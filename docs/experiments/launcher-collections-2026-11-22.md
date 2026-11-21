<!-- SPDX-License-Identifier: Apache-2.0 -->
# Launcher collections — 2026-11-22

The authorized OrbStack/Web host now presents Public as The Brass Relay, OIB
Designer and Test Playground. Home has no self tile. Playground shows Components,
Layouts, Long Document, Text Panning, Text Input, 2D Animations, 3D Animations and
Bundle Counter. Monitoring remains in its separately authorized Operations Space.

The shared Rust launcher derives placement from signed catalog paths and filters
only presentation. Discovery/admission, attachment and independent datasets are
unchanged. Missing attached parents expose their admitted children at the next
available level. Collections reuse Home's icon tiles, scrolling and management
controls. Edit retains ordering, app information, sharing, export/replacement,
removal and access to retained data. Back and direct navigation use the same
hierarchy. Legacy Designer and Counter links continue to resolve.

Source-owned routes come from authored app navigation; the registry composer
accepts `--routes apps/launcher-routes.json` for the Designer reparenting and the
independently installed Counter. Complete builds preserve that input. Existing
installers preserve paths from the active catalog. The standalone authored
Playground preview retains its original seven-field ordering schema; managed
Spaces render their actual admitted children through the shared launcher.

## Validation and deployment

Built with pinned Rust 1.90.0:

- `cargo test --locked -p otrios-core --features canvas --test spaces_ui
  launcher_collections_preserve_admission_management_and_parent_navigation`:
  passed. Covers Home filtering, collection membership, child management controls,
  return navigation, detached-parent fallback and direct-link startup before
  discovery arrives.
- `cargo test --locked -p otrios-core --features canvas --test spaces_ui
  dropping_an_app_on_a_later_tile_reorders_without_launching_or_rebinding`:
  passed; the existing drag regression checks the reused ordering controls.
- `cargo build --locked -p otrios-dashboard --features canvas,server,host-sync
  --bin canvas-server --bin canvas-bundle --example space_map_probe`: built on
  Linux aarch64. Mac packer/checker builds also completed.
- `bash scripts/build-dashboard-web.sh`: completed the source app pack, release
  Wasm build and wasm-bindgen generation.
- `cargo build --release --locked -p otrios-dashboard --features direct,server,canvas
  --bin dashboard-direct --target armv7-unknown-linux-gnueabihf`: completed in the
  pinned ARM cross-builder.

Signed release:
`3d2531716f051fb9ab0d3382dd0632b50022849247b978322909596c8098a68d`.
The complete manifest, asset digests and nested bundle signatures were verified
before atomic activation. All 13 app state/execution contracts match the previous
release. All 73 authority files outside Home and Playground remain byte-identical,
including Designer and its independent libraries. No datasets were detached,
removed, migrated or reset. State protocol v40 and release-control v1 are unchanged.

A read-only public WSS journey using the real shared client confirmed exactly the
three Home destinations, all eight Playground experiments, a confirmed Bundle
Counter mount, Back to Playground, Back to Home and direct collection navigation.
The probe sent no edits. Public HTTP checks returned 200 for canonical and legacy
links, 404 for retired Live UI and 401 for anonymous `/auth/session`. Authority,
tunnel and Monitoring timer remained active.

The prior signed release `3c3d1a646c733a5e80192e2e266f752a467650921285e769b3a33acaf80d7486`
and compatible authority executable remain available for development rollback.
No PR or remote push was made. Broad preflight suites were intentionally deferred
under the focused live loop. This records protocol/scene acceptance, not browser
visual or physical Frame acceptance; Frame remains offline. The Designer
appearance consolidation remains separate work described in [DESIGN.md](../../DESIGN.md).
