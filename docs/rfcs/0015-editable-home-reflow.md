<!-- SPDX-License-Identifier: Apache-2.0 -->
# Editable Home and local reflow

Scope: six existing Home cards, with a reusable bounded reorder presentation.
Home's UI document describes the wrapping flow and maps stable state field IDs
to component nodes. Card contents and navigation stay authored components.

Each card has one writable text position register. Sort lexicographically by
position, then stable field ID. A move writes only the moved card's fractional
position between its new neighbors, including an identity suffix. No array,
other card positions, coordinates, editing flags or animation frames are written.
Different-card moves retain both writes; simultaneous moves of the same card
use the existing authority's serialized register policy. This is a bounded
position-register sequence, not a general sequence CRDT or collection API.
Malformed positions fall back to the schema default. Position growth is bounded;
an exhausted interval rejects the move instead of rewriting the collection.

Edit/Done, pointer capture, drag offset and wiggle belong to the local mount.
Only crossing a different insertion slot emits a position edit. Repeated samples
in the same slot emit nothing. Cancel releases the gesture and retains moves
already shared. Editing suppresses launching cards; keyboard arrows move a
focused card. Orientation changes cancel capture before local layout changes.

A wrapping flow fits as many minimum-width cards as the available width permits,
shares remaining row width and preserves each component's interior pins. Shared
position changes update target frames. A separate presentation owner interpolates
those frames and wiggles only the editing client's cards. Its clock never ticks
the document. Both browser and native use the same runtime presentation code.

Wrapping compositions use layout v4; earlier layout versions remain readable.
Home starts a new state reference because its previous schema was empty. Other
app roots are unchanged. State transport stays v4: positions use existing typed
text writes. Release-control v1 remains independent.

Acceptance: two clients observe live reordered cards in portrait and landscape;
Edit/Done and pointer motion within one slot generate no shared edits; card IDs/focus survive
moves; cancel/resize suppress accidental navigation; independent concurrent moves
converge; repeated delivery is idempotent. Run the repository checks, browser
interaction checks, and native cross compilation before automatic deployment.
