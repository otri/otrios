<!-- SPDX-License-Identifier: Apache-2.0 -->
# Shared navigation and appearance — 2026-11-22

The authorized OrbStack/Web host now uses one shared Rust shell for the launcher,
collections, authored apps, Designer and Monitoring. Breadcrumbs omit Spaces:
`OTriOS > App` or `OTriOS > Test Playground > Experiment`. Ancestors retain Space
selection and admission, and navigation preserves existing Designer leave guards.
The canonical inventory is in [DESIGN.md](../../DESIGN.md). The obsolete
`/bundle-counter` and `/test-playground/oib-designer` aliases now return 404;
retired Live UI remains absent.

## Delivered behavior

- Shared top row, divider and account placement in both appearances. Room-specific
  participant/identity controls remain in chat. App commands sit below the global
  row. Home/collections use “Arrange apps”; permissions, ordering, sharing, data,
  export/replacement and removal remain available through existing management.
- Brass Relay charcoal/brass/copper in Dark; warm paper, ink and Home identity
  colors in Light. Interactive boundaries differ from quiet decorative dividers.
  Designer chrome, dialogs, fields, buttons and segments share the paired palette
  and 12-unit control radius. OIB Designer has a distinct launcher icon.
- System/Light/Dark in the shared account menu. Browser preference is local and
  persisted independently of app state; System follows the OS media preference.
  Native defaults to Dark without a host appearance signal. Theme changes defer
  during gestures, preserve the current selection/drafts and write no app values.
- Designer keeps global location separate from document/component scope and save
  status. “Preview appearance” remains local to the authored preview; changing
  shell appearance does not overwrite explicit authored colors or preview mode.
  Account settings and sign-out retain unsaved-change guards. The account popup
  blocks underlying pointer and semantic actions while retaining raw drafts.
- Monitoring fits its existing 16:10 dashboard below an independently anchored,
  normal-size header. Chart layout remains fixed; chart colors resolve for Light.
- Existing admitted Roboto resources are unchanged. The breadcrumb separator is
  ASCII `>` because the current packed font does not include U+203A.

This implements shared presentation for the current catalog, not the entire
future control/style contract in RFC 0037. Longer/deeper breadcrumbs need overflow
or reflow before adding them; current paths fit the supported landscape shell.

## Build and focused checks

Pinned Rust 1.90.0, with no new dependencies:

- `cargo test --locked -p otrios-core --features canvas --test spaces_ui
  launcher_collections_preserve_admission_management_and_parent_navigation`:
  1 passed. Includes admission/management/parents, aliases, account barriers,
  local appearance, pressed feedback and deferred appearance coalescing.
- `cargo test --locked -p otrios-core --features canvas --lib oib_leave::tests`:
  3 passed. Apply confirmation, rejection/discard and Stay behavior retained.
- `cargo test --locked -p otrios-core --features canvas --lib
  shell_appearance_and_account_preserve_invalid_draft_and_preview_mode`:
  1 passed. Raw invalid text, selection, focus, preview mode, popup input barriers,
  late identity discovery and account-settings leave handling.
- `node --test scripts/test-canvas-worker.mjs`: 18 passed.
- Mac `canvas-bundle`/`canvas-pack` and Linux aarch64 `canvas-server`/`canvas-bundle`
  built with `--locked -p otrios-dashboard --features canvas,server,host-sync`.
  `bash scripts/build-dashboard-web.sh` completed source packing, release Wasm and
  wasm-bindgen. The installed signed app bundles were retained when publishing.
- `cargo build --release --locked -p otrios-dashboard --features direct,server,canvas
  --bin dashboard-direct --target armv7-unknown-linux-gnueabihf`: passed in the
  pinned cross-builder. Output verified as ARM EABI5 ELF32.

Broad preflight suites were intentionally deferred under the focused live loop.
No test changes application wire format: state protocol v40 and opening
release-control v1 remain unchanged.

## Signed deployment and live acceptance

Active release:
`cb2f5c813921d6c4a89edac34b2c7c6cde899449b80b346d88b1bbaa7976db67`.

The complete manifest, asset digests and nested signatures were verified on the
publisher and authority before locked, atomic activation. All 85 authority files,
all app contracts and the signed catalog are byte-identical to the baseline.
Existing datasets, authentication configuration and independent library revisions
were retained. The existing authority service was rebuilt/restarted; no new
service, account or trust root was introduced.

After activation, a read-only real public WSS journey using the candidate shared
client mounted all 12 public catalog entries. Both appearances had identical
semantic control labels and checked states, unchanged inspected app values and
Designer resource usage. All public account menus exposed the same appearance
choices and dismissed correctly. No edits were sent. The private Monitoring UI
was separately rendered with the installed signed UI and existing private feed:
its header remained at y=24 and its 16:10 canvas fit at 1232 × 770 within a
1280 × 1100 viewport. This local rendering is not authenticated live acceptance.

All 13 canonical HTTP routes returned 200. Both removed aliases and retired
Live UI returned 404. Anonymous `/auth/session` returned 401, and an anonymous
private Monitoring mount was denied without delivering its UI payload. Public
release discovery matched the activated hash; downloaded JS, worker, CSS and
Wasm matched the verified manifest digests. Authority, tunnel and Monitoring timer
remained active. The Mac publisher pointer was promoted only after these checks.

Representative exported core scenes received an SVG layout review. This is
scene/protocol acceptance, not production browser/GPU pixel or physical Frame
acceptance. Frame remains offline; its ARM build is available but was not installed
or exercised. Authenticated login was human-confirmed before this change and was
not repeated here; authentication itself is unchanged. Complete control-state,
contrast and long/localized-label qualification remain follow-ups.

The compatible baseline signed release
`3d2531716f051fb9ab0d3382dd0632b50022849247b978322909596c8098a68d`
and its authority executable remain available for development rollback. Private
check logs, scene captures and probe source are retained under the existing
OTriOSOrbStack support directory. No remote push or PR was made.
