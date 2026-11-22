<!-- SPDX-License-Identifier: Apache-2.0 -->
# Relay dialogs on phones — 2026-11-22

Implementation commit: `c429ff3`.

The calling-card dialog used desktop footer widths, putting Cancel outside a
narrow panel. Its title also painted through Close, while the handbook painted
single-line instructions beyond the panel edge.

Dialog headings now wrap at measured glyph widths, use readable phone typography,
and reserve space for Close. Calling-card actions share the available width,
with a short Save caption at the narrowest sizes. The redundant visible “Your
room name” label is removed. Guest identity information sits below the field;
name/password field paint is clipped and caret windows use the actual field width.

The handbook has a clipped, word-wrapped body with local touch/momentum and
wheel scrolling. Close and the action footer remain fixed. Narrow footer actions
stack when there is enough height and use shorter captions in short layouts.
The last command remains reachable. Closing clears scroll state, a captured pan
blocks reload, and the admin menu blocks wheel input to the handbook beneath it.
No chat edits, new services, dependencies or wire-format changes are introduced.

## Validation

- `cargo test --locked -p otrios-core --features canvas --test canvas
  brass_relay_phone_dialogs_keep_actions_inside_and_wrap_and_scroll_the_handbook`:
  1 passed. Checks 320/390/430-wide phones and a short landscape layout, in-panel
  actions, heading clearance, removed copy, clipped long-name entry, modal input
  barriers, measured wrapping, touch/wheel scroll, final-command reachability,
  fixed actions, unchanged mount and no chat writes from panning.
- The existing `brass_relay_qualifies_crowded_roster_avatar_pages_and_native_keyboard_in_both_orientations`:
  1 passed. Its two stale entry labels were updated to Participants/Chat identity.
- Mac tooling, release browser Wasm, Linux authority/checker and ARMv7 host builds
  passed with the pinned toolchain. No full suite was run.

Signed release:
`45fe02ce90cbe6a26a041ff5ae9f1aa9a37452b24b005add96bd8569bcf98b14`.

Full signed closure verification passed on Mac and Linux. All 85 independent
installed authority files, app contracts and catalog bytes are unchanged from
`9e41291c49adb4bd532250768782e49cfd9a555e503048eb30d7dcdd00980d21`.
The compatible previous authority executable and signed release remain available
for rollback; the restored publisher pointer matches the active host.
Activation first met a startup 503 after restart, before changing the pointer;
the retry succeeded once the authority was ready.

The real public browser automatically loaded the new signed release at
390 × 720 CSS pixels. Calling-card field/actions stay inside the panel, with only
one active text input. The handbook exposes Close, Dispatches and Send file inside
its bounds; wheel input produces no application error and leaves these controls
fixed. A supplemental synchronous CLI WSS probe timed out before mounting Relay
and is not counted as acceptance. Its logs are retained with the successful
regression/build/browser evidence under `~/Library/Application Support/OTriOSOrbStack/`.
Physical iPhone Safari pixels were not retested. Authority, tunnel and Monitoring
timer remain active; the disposable builder is stopped.
