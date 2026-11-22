<!-- SPDX-License-Identifier: Apache-2.0 -->
# Relay light appearance and rounded fields — 2026-11-22

The reported light-mode transcript used almost-white text on the paper surface.
Chat UI primitives received the shell palette, but the retained document layer
had already packed its own glyph colors and did not receive that update.

Chat mount invalidation now supplies the shell foreground to the document reader.
When it changes, the reader retires paint-bearing pages and repacks through its
existing bounded work/allocation path. Width measurements, wrapping checkpoints,
source identity and reading position remain. Held frames retain their immutable
old meshes; the new document epoch invalidates GPU resources on both hosts.
Inline icon geometry retains its authored colors. The app/transcript data and
state protocol are unchanged.

The composer combined a Canvas exponent-4 squircle with an HTML editor's circular
12-unit corner. Chat button/field backgrounds now use matching circular outer
12-unit and inset 10.5-unit corners. The browser's opaque editor and inset border
cover the same curve. The primary button boundary follows the shared action role
in Light instead of retaining a literal pale-gold border. HTML placeholders now
use the paired muted text color with full opacity instead of the browser default.

## Focused validation

- `cargo test --locked -p otrios-core --features canvas --lib
  foreground_repaint_preserves_anchor_measurements_and_held_meshes`: 1 passed.
  Checks actual glyph RGBA in Light/Dark, epoch changes, retained measurement and
  anchor/offset, idempotent color selection and immutable old frames.
- A public WSS shared-client journey cycles Home → Relay → Home three times with
  phone viewport feedback and two cached UI hits. It switches Light/Dark/Light,
  waits for the real transcript chunks and checks their glyph foreground values
  plus circular composer geometry. No edits are sent.
- Pinned Rust 1.90 builds: Mac packer/checker, release browser Wasm, Linux aarch64
  authority/checker and ARMv7 direct host. No new dependencies, broad preflight or
  protocol changes.

Signed release: `e1856383894a4a70bb6875c22fb5423bb03b454365c1ee6a2f30e55d8b82c493`.

The complete closure is verified before atomic activation on the existing
OrbStack/Web host. Signed catalog, app contracts and all 85 authority files are
retained byte-for-byte from the baseline; rollback code and release remain.
Physical iPhone Safari corner/interaction acceptance remains a follow-up on the
reporting device. Browser check results are recorded after activation below.

After activation, the public WSS journey passed three cycles again with two
cached mounts and verified Light/Dark/Light glyph RGBA. The existing browser tab
automatically loaded the signed release above at 390 × 720, stayed in Light and
reported no application error. Computed editor radius was 12px; placeholder color
was rgb(109, 116, 105) with opacity 1. Preview pixel capture failed because the
browser produced no screenshot frames; the permitted native-capture fallback
timed out. This records browser load/DOM and actual core glyph-paint acceptance,
not a completed browser pixel or physical iPhone visual qualification.
