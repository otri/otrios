<!-- SPDX-License-Identifier: Apache-2.0 -->
# App-owned navigation acceptance

Home opens at `/` and owns the names of its app destinations. Layout Lab owns
its relative `signal` and `cards` names, producing `/layout-lab/signal` and
`/layout-lab/cards`. Named navigation edges live in the owning UI documents;
the packer derives catalog paths by walking those edges. Package identities,
UI references and State bindings remain independent of route names.

The browser and native share the core catalog resolver and parent Back behavior.
The browser adds generic URL/history plumbing. `navigation.mjs`, hash aliases and
release-entry redirects are removed. Each HTTP screen entry remains no-store,
with executable assets pinned internally to a signed immutable release.
Release-control v1 and state sync v4 keep their existing wire contracts.

Qualification passes: 185 Rust tests (including seven native worker checks),
40 Python checks, 49 JavaScript/Wasm checks, all-feature ARM Clippy and the ARM
release build. Real process tests verify child screens sharing one persisted
State root and returning to their parent. Packing rejects missing route owners,
duplicate sibling names and cycles.

Chromium verifies direct child links, refresh, browser Back/Forward, parent Back,
private admission, local keyboard/history isolation, shared input and both
orientations. A full runtime update reloads `/layout-lab/cards` at the same history
position. A UI-only update preserves pages, sessions, focus and confirmed state.

A separate signed UI-data test changes Home's `layout-lab` name to `layouts`.
An active Cards screen moves to `/layouts/cards` with the same UI, State, epoch,
page and WebSocket. Its history length stays fixed. The owning Home UI also
hot-swaps, and parent Back, launch and refresh use the renamed path. The child
UI document does not change. Production retains the original `layout-lab` name.

Deployed release: `d9fa55ceab9782ef59b54d849b09730b448c479f862e87a066207f70a09a8e10`.
All 80 public HTTPS checks pass. The real native renderer is connected and
confirmed at `app:home/state-v3`, with its previous order `[1, 4, 2, 5, 3, 6]`,
800×1280 output and rotation 90. Nine screen documents retain seven durable State
roots. This release rebuilds the Wasm/native hosts; later compatible UI/route
edits can reuse this qualified runtime. No attended physical touch acceptance
was performed for the new child screens; native navigation is covered headlessly.

See [machine-readable evidence](semantic-navigation.json) and
[screen URL behavior](../HOME_AND_INPUT.md#shareable-screen-urls).
