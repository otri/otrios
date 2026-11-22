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

## Scrolling user list

Commit `eb83a47` replaces the roster's eight-person pages with one scrollable
list covering all 68 admitted members. Only rows intersecting the viewport are
painted, keeping avatars and controls within the existing scene limits without
Previous/Next buttons. The summary and member names wrap to their measured
available widths. Short rosters size the panel to their content instead of
leaving a nearly empty full-height dialog. Close stays fixed, as does the legacy
admin sign-in action where applicable.

The roster reuses the handbook's local wheel/touch/momentum scrolling. Clipped
row portions remain clickable. A drag never opens a member; a tap captures the
member's identity, so reordering the roster before release cannot select a
different person. No chat writes are sent by browsing. Long Unicode labels are
bounded by UTF-8 bytes and wrapped-name lines use separate scene ID allocations.
Retired paging command numbers remain harmless for stale queued controls.

`cargo test --locked -p otrios-core --features canvas --lib
roster_scrolls_all_members_with_bounded_scenes_and_stable_tap_identity`: 1 passed
(591 filtered). The fixture walks all 68 members at 320 × 568, 390 × 720 and
1280 × 800, including maximum-length wide Latin/Japanese names. It verifies scene
admission, bounded visible controls, clipping, name wrapping, absent paging,
stable tap identity after reorder, drag behavior, no writes and compact one-user
sizing. No full suite was run. Mac tooling, browser Wasm, Linux authority/checker
and ARMv7 host builds passed.

Signed release:
`f384a90d9456076c2ac3fb97d56fa21eecff902bdc70ce935e3d498346afd071`.
Full Mac/Linux closure verification passed; all 85 independent authority files,
app contracts and catalog bytes are unchanged. The prior signed release and
compatible authority executable remain available for rollback.

The public browser automatically loaded this release at 390 × 720 CSS pixels.
Participants opens the compact one-user roster, with its row and Close inside
the panel and no paging buttons. Selecting the user opens their details without
an application error; no salute or profile change is submitted. Overflow and
68-person behavior are covered by the focused fixture, not a live crowded room.
Physical iPhone Safari was not retested. Authority, tunnel and Monitoring timer
remain active; the disposable builder is stopped.

## Modal focus stays inside the dialog

Commit `3873be2` fixes the empty outline over the roster heading. The browser's
modal auto-focus chose Back to OTriOS because background controls were still
included in its accessibility control list. Its transparent button displayed a
focus outline at [24, 24, 78.84, 48], through the modal paint.

Relay now marks its modal barrier explicitly and clears background responders
and semantics after composing the shared navigation. Background paint remains
unchanged. Accessibility text excludes the covered backdrop. A local frame flag
identifies every Relay dialog, including those without scrolling, so browser
focus trapping and dialog labeling apply consistently. Keyboard focus indicators
remain enabled for the controls inside the dialog.

The extended existing `brass_relay_phone_dialogs_keep_actions_inside_and_wrap_and_scroll_the_handbook`
regression passes (1 passed, 69 filtered). It verifies no background control can
receive focus or stale activation, background text is not announced, name entry
and handbook scrolling still work, and navigation returns after closing. Mac
tooling, browser Wasm, Linux authority/checker and ARMv7 host builds passed; no
full suite was run. The frame flag is local presentation metadata; shared state
and the release bootstrap protocol are unchanged.

Signed release:
`0dc8adaf77cea0e08d8d85dbd9b8dff9a56a897b40af83960f89a8deaf440f96`.
Full Mac/Linux signed closure verification passed, with all 85 independent
authority files, app contracts and catalog bytes unchanged. Compatible rollback
remains available and the restored publisher pointer matches the active host.

The real browser reproduced focus on Back to OTriOS before deployment at
390 × 720. After automatically loading the signed update, opening Participants
exposes only Close and the user row, focuses Close dialog, labels the group as a
Relay dialog, and announces only dialog text. Closing restores normal navigation
without an application error. This qualifies the browser DOM/focus behavior;
physical iPhone Safari pixels were not retested. Authority, tunnel and Monitoring
timer remain active; the disposable builder is stopped after builds.
