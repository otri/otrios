<!-- SPDX-License-Identifier: Apache-2.0 -->
# Visual refresh and keyboard reflow — 2026-10-27

Raw measurements and figures from this report were retired from the current tree.
The [original report and artifacts](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/visual-refresh-2026-10-27.md) remain available at the pinned historical revision.

Home and The Brass Relay now share outlined charcoal surfaces, brass actions,
copper icons and consistent font metrics. The lounge header reads
**< OTriOS • The Brass Relay**, with matching sizes and baselines. The calling
card uses a mixed six-column avatar grid, clear field labels and secondary Cancel
beside the brass confirmation action. All 64 avatars have distinct geometry;
they use shared native/browser vector primitives without new dependencies.
The rules are recorded in [the visual language](../VISUAL_DESIGN.md).

Opening a browser keyboard now reflows chat height at its existing horizontal
scale. It no longer fits the entire tall scene inside the reduced height.
The canvas width is preserved immediately, before the worker acknowledges the
new extent. The editor retains its value, focus and selection. Short viewports
reduce visible composer rows and compact the profile dialog. Other demo layouts
keep their existing sizing; browser pinch zoom remains available.

## Qualification

- `bash scripts/check.sh` passed.
- The signed build passed its Rust checks and all 69 Node tests.
- Core regressions cover keyboard-height layouts, heading alignment, control
  bounds, avatar paging and the retained particle scene. Icon checks cover 64
  unique drawings and names, finite coordinates and reserved node budgets.
- `scripts/test-chat-browser.mjs` passed against the exact staged signed release
  on a disposable local authority. It covers identity, live drafts, messages,
  avatar paging, native browser editing/IME, wrapped input, dialogs, graphics
  recovery and viewport-only keyboard resizing.
- The keyboard regression holds the layout viewport at 390×844 while changing
  the visual viewport to 390×420 with a 50-pixel top offset. Canvas width remains
  390 pixels both immediately and after reflow; focus, value and selection remain
  unchanged. This is a simulated viewport event, not an automated iOS keyboard test.
- The user's subsequent iPhone screenshot shows the deployed lounge filling
  the available width with the system keyboard open and the composer visible.
  Keyboard animation timing and all iOS editing gestures remain manual coverage.
- Live native readiness confirms the signed executable hash, confirmed chat
  state, stream readiness and zero pending commands. Authority, display and
  tunnel services are active. Public release assets match the signed manifest.

Evidence: [browser acceptance](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/assets/visual-refresh-2026-10-27/acceptance.json),
[viewport measurements](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/assets/visual-refresh-2026-10-27/mobile-reflow.json),
[native verification](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/assets/visual-refresh-2026-10-27/device-verification.json)
and [public asset verification](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/assets/visual-refresh-2026-10-27/public-verification.json).

[Archived figure: Calling card](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/assets/visual-refresh-2026-10-27/profile-icons.png)

[Archived figure: Home](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/assets/visual-refresh-2026-10-27/home-landscape.png)

[Archived figure: Simulated phone keyboard viewport](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/assets/visual-refresh-2026-10-27/phone-keyboard-reflow.png)

## Deployment

Activated release:
`e610e281164c083987391c7f8760e3dc8d666d770fa4cf8238b20dcced1c1dcf`.
Native executable SHA-256:
`95b76b5aef63728b446e1084f24720c702cd2eb32dc72537645aec590cc8b2e1`.
Touch calibration remains
`44f2b33a3a17140e6c662b9feba1f009c9ed7910f2c1fa2178b7f5a24a53859e`.

The previous signed code release and service binaries are retained at
`/var/lib/otrios-authority/rollback/2026-10-27-visual-refresh` for code rollback.
No chat history backup or state protocol change was needed. Release discovery
keeps its v1 opening envelope and chat remains on state protocol v6.
