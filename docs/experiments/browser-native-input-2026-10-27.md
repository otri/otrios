<!-- SPDX-License-Identifier: Apache-2.0 -->
# Browser-native input bridge — 2026-10-27

Raw measurements and figures from this report were retired from the current tree.
The [original report and artifacts](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/browser-native-input-2026-10-27.md) remain available at the pinned historical revision.

The browser previously suppressed the frame keyboard without supplying an HTML
editing surface. Canvas focus alone could not open the iPhone system keyboard.
The browser now mounts real input/textarea elements over the authored fields;
focus happens directly during a user gesture. The native frame keeps its existing
primitive keyboard and text viewport implementation.

Committed values and UTF-16 selections pass through the shared Rust editor's
byte limits and grapheme validation. Provisional IME text stays in the browser;
composition blocks a hot code replacement. Revision acknowledgements protect
unacknowledged typing and native selection from delayed frames. Epoch, visible
field and focus validation reject stale events. A selection-only notification
cannot restore obsolete text after a remote edit. Native textarea scroll offsets
also position the shared typing sparks; there is no second hidden caret-follow
animation competing with the HTML viewport.

The browser uses the existing licensed Roboto source with the same logical font
metrics. Its immutable font and OFL notice are in the signed closure; the font
has a precompressed response and participates in the public artifact cache.
This adds no dependency or state protocol change. It does not extend canvas
shaping, bidirectional layout or the existing glyph subset.

## Validation

- `bash scripts/check.sh` passed, including workspace formatting, Clippy, Rust
  tests, server process tests and release/deployment checks. These remain
  dashboard/scaffold checks, not acceptance of the proposed Linux shared-memory IPC.
- The signed builder passed 66 Node tests. New cases cover UTF-16 selections,
  composition privacy, delayed frames, queued native selection, obsolete values,
  acknowledgements after rejected input and composition upgrade gating.
- `scripts/test-chat-browser.mjs` passed against the staged signed public release:
  native textarea focus, selection replacement, Chromium IME composition,
  Shift+Enter newline, Enter send, multiline text, two guests, live/private drafts,
  avatar/name editing, reload identity and WebGL recovery.
- `scripts/test-responsive-browser.mjs --input-viewport` passed: two-client text
  updates, native Tab focus, byte-budget rejection/reconciliation, horizontal
  input scrolling and reduced mobile viewport fitting. Browser editor bounds
  matched authored control width with **0 CSS-pixel difference**; computed Roboto
  size was 22.5055 px before scene scaling.

The browser tests present each tab before waiting for rendered state. A background
Chromium tab can withhold animation-frame presentation and its single frame
credit; that is distinct from state transport completion. Native key tests include
the Enter character event, so Shift+Enter exercises the browser's default newline.

The mobile check uses a 390×844 viewport reduced to 390×420 to represent available
keyboard space. It is **not a physical iPhone or an iOS software-keyboard test**.
Actual iPhone keyboard activation, selection handles, suggestions and keyboard
animation still need device verification. Computed field fonts remain at least
16 px and scale with the scene; pinch zoom is not disabled.

[Chat acceptance](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/assets/browser-native-input-2026-10-27/chat.json) and
[field acceptance](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/assets/browser-native-input-2026-10-27/fields.json) record the
browser checks. The [wrapped composer](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/assets/browser-native-input-2026-10-27/wrapped-composer.png)
shows the real textarea in the lounge. Screenshots are a visual review, not a
pixel-equivalence claim between browser and GLES font rasterization.

## Deployment

Activated signed release
`35d47b1a06a8635cc1159782e9414531893d94a7125e4feaa6158082c0a0ed42`
on the existing frame authority and public web host. The final source snapshot
reproduced the same release tested by the browser acceptance scripts.

The running frame executable SHA-256 is
`295b2e34b2e0c2c5c9fbe3b878f11442691037b2d1d67335680b139a96764c9a`,
matching the signed manifest. Authority, display and tunnel services are active;
the native client reports confirmed state, a ready stream and no pending commands.
GLES2 direct DRM, rotation 90, 800×1280 RGB565 and the accepted touch calibration
remain active. A code-only rollback copy is available at
`/var/lib/otrios-authority/rollback/2026-10-27-browser-input`; current test chat
history is disposable and was not backed up for this deployment.

The public HTTPS release manifest, editor module and Roboto font were fetched and
matched to their admitted SHA-256 values. [Device readiness](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/assets/browser-native-input-2026-10-27/device.json)
and [public asset verification](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/assets/browser-native-input-2026-10-27/public.json)
record the deployed checks. No PR was opened pending device feedback.
