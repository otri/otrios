<!-- SPDX-License-Identifier: Apache-2.0 -->
# Screen URL acceptance

Screen links now use `/home`, `/component-playground`, `/input-lab` and the other
catalog app IDs. One client router owns parsing and browser history. Keyboard,
focus, editing and dragging stay local. Each screen request receives a no-store
entry page whose script and stylesheet name one immutable release internally.
Release-control v1 and state sync v4 retain their existing descriptors and codecs.

The required repository checks pass: 180 Rust tests, 39 Python checks and 51
JavaScript/Wasm checks. All-feature ARM Clippy and the native release build pass.
HTTP process tests verify catalog-based entry routing while state recovery fails,
unchanged immutable HTML fetches, expired release bookmark redirects, unknown
screen rejection, and private/API boundaries.

Real Chromium acceptance covers direct links, refresh, Back/Forward without
page/session replacement, unchanged history when opening the keyboard, module
release identity in the opening hello, legacy hash/release links, and Monitoring
sign-in at its own URL. Existing responsive/shared-input checks also pass.

A signed runtime-closure update reloads `/layout-lab` at the same history position
with a new module release. A subsequent app-data-only update retains `/home` and
`/component-playground`, their pages, WebSockets, focus and state; refresh then
loads the current release at the same screen path.

The deployed release is `af6e06ae315f012ad06e44b26cddc68470494765c35ac2a3fbd10d4fb825ccfa`.
All 76 public HTTPS checks pass. The native frame remains connected and confirmed
at `app:home/state-v3`, preserving its shared card order `[1, 4, 2, 5, 3, 6]`.
Host Wasm, the native executable and app packages reuse the qualified Home build;
the HTTP server, web host scripts and signed asset allowlist carry this change.

See [machine-readable evidence](semantic-navigation.json) and
[screen URL behavior](../HOME_AND_INPUT.md#shareable-screen-urls).
