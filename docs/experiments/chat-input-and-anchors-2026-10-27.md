<!-- SPDX-License-Identifier: Apache-2.0 -->
# Chat input and viewport anchors — 2026-10-27

## Corrections

Wasm disables the runtime software keyboard through a host-local input policy.
Editor focus and physical keyboard/paste handling remain active. The native host
retains its software keyboard. The setting is not replicated room state and
does not change the session/state protocol.

The composer wraps on grapheme boundaries, grows to four rows, and uses the
existing timestamped drag/coast physics to preview longer text vertically. Edits
return the caret to view. Cached wrapping supplies painting, hit testing and
effect positions; it does not run again for an unchanged string/width/font.
Live draft previews wrap around the edited range rather than truncating a single
line. Composer controls keep their identity when draft rows change.

Typing sparks now resolve a source-byte anchor against the same wrapped layout
and viewport offset used to paint the text. Remote replacement anchors exclude
the common unchanged suffix. Offscreen/unshown drafts do not emit elsewhere;
unchanged lease renewals and caret-only movement do not emit typing sparks.
Particle trajectories are culled rather than clamped to a misleading origin.

The avatar defect was a stale presentation cache: pointer movement changed the
document offset without invalidating chat's avatar overlay. That overlay now
refreshes before the next scene read, even without a clock tick. Ordinary long
documents retain their static UI tree during scrolling. Stable message blocks
sharing one placement/clip for text and avatars remain the intended next step.

## Sync direction

[RFC 0024](../rfcs/0024-viewport-scoped-collections.md) describes coherent bounded
collection views on the existing partial Merkle synchronization foundation.
Stable keys/cursors select relevant records and scrollback; local pixel offsets
never become replication selectors. The current separate chat journal/history
transport remains in use. The proposal does not claim a completed CRDT history
migration or require clients to retain the entire log.

## Validation

The required `bash scripts/check.sh` suite passes, including warning-free Clippy
and process tests. All 55 Canvas integration cases and three document cases pass.
The new cases check browser keyboard policy with retained editing, bounded
wrapped composition, avatar/text movement before an animation tick, exact local
and remote replacement spark origins, lease renewal suppression, stable control
identity, source boundaries and wrapped hit testing. Existing native keyboard,
chunk retention and scoped authority tests remain active. No dependency added.

Two independent real Chromium/WebGL contexts passed the signed-artifact checks,
including absent browser software keys, wrapped composer growth and caret
visibility, messages, live/private drafts, rename/avatar selection, multiline
paste, reload, responsive layout and graphics loss/recovery. The
[wrapped composer reference](assets/chat-input-and-anchors-2026-10-27/wrapped-composer.png)
was visually inspected.

## Activation

Release `8d9e948dc9c89dee6398e4d96216b327e2cbb6acc9abd3a3cb47f00a4e53e134`
is active on both web listeners and the native last-good pointer. The running
native executable matches signed SHA-256
`b57501988bad892a326da4dd9e5c0c1e2c631c78484df409c8edec0b8939412a`.
Native is stream-ready and state-confirmed with zero pending commands; authority,
display and tunnel services are active. The touch calibration hash is unchanged.

Rollback at `/var/lib/otrios-authority/rollback/2026-10-27-chat-input-fixes`
includes the preceding signed release and a consistent copy of the live chat/app
state. [Machine-readable qualification](assets/chat-input-and-anchors-2026-10-27/qualification.json)
records the browser checks and activation. Native physical interaction remains
available for user testing. No PR was opened.
