<!-- SPDX-License-Identifier: Apache-2.0 -->
# Home and reusable input

Home is an admitted Canvas launcher with three compact cards: **The Brass Relay**,
**Test Playground** and **Monitoring**. Home and the lounge share neutral charcoal
surfaces, restrained brass accents and the standard OTriOS heading. Each card contains an icon, title and subheading. Test
Playground owns the seven demo destinations: Components, Layouts, Long Document,
Text Panning, Text Input, 2D Animations and 3D Animations. Layouts has no extra
links to the document or panning demos. Demo headers provide Back to their parent
and Home to the launcher. Monitoring retains its private admission boundary.

Navigation and route history stay on the device. A candidate Canvas qualifies
UI, behavior and selected state before accepting its lease; a rejected candidate
retains the current activation. The host does not select a compiled screen UI.
Friendly route aliases remain bookmark conveniences over admitted app identities.

Input Lab stores two shared atomic text registers. The reusable primitive keyboard
and field bindings live in the app package. Caret, selection, focus, keyboard page,
repeat timing and previews stay local in RAM. A tap chooses the caret from actual
font advances, grapheme boundaries and the displayed text window. Switching fields
and repeated keys use the same responder/input implementation on both hosts.

On the native frame, text uses the full width between the field's 24-unit inner margins, with measured
font advances and a clipped pixel viewport. Dragging pans independently of the
caret, coasts on release and stops at either text boundary without overscroll.
A tap places the caret in the panned text. Typing, deletion and left/right arrows
quickly bring the caret back into view with up to 48 logical units of side context;
idle frames and remote updates do not pull a manually previewed viewport back to
the caret. Focus changes, cancellation and UI replacement stop active gestures.
Viewport position and momentum remain local in RAM.

Browsers use real HTML inputs/textarea over the authored field bounds. A tap
focuses the element directly in the user gesture, allowing the iPhone system
keyboard and native selection, cut/copy/paste, autocorrect replacement and IME.
Browser fields own their visible text, caret, selection and scrolling; the native
frame retains the shared primitive keyboard and momentum preview behavior above.
The browser face is the same licensed Roboto source, with shared logical metrics,
kerning and ligatures disabled to match the current renderer. The viewport fits
the visible area above a mobile keyboard without flipping the scene orientation
just because the keyboard reduced its height. Pinch zoom remains enabled.

Full browser value/selection snapshots pass through the same bounded Rust editor.
UTF-16 offsets are converted to complete UTF-8 grapheme boundaries. Invalid control
characters, split surrogates and over-budget replacements are rejected atomically;
the acknowledged editor value restores the DOM. Intermediate IME text stays local
to the browser until composition ends. Local revision acknowledgements prevent old
worker frames from overwriting newer typing or selection. Selection-only messages
are rejected if their base text is obsolete, so moving a caret cannot undo another
client's text edit. Mount, visible field
and focus checks reject late input after navigation or a modal change. Composition
also blocks hot replacement until committed or discarded on unmount.

Text Input still uses shared atomic registers, rather than a sequence CRDT.
Selection, composition and field viewport state remain local. The native frame
uses its existing accepted touch calibration. A code reload retains confirmed text
through server state and restarts local focus. Accepting Unicode input does not
extend the canvas renderer's glyph subset, shaping or bidirectional support.

See [app sources](../apps/README.md), [scene routing](SCENE_TREE.md),
[storage](CLIENT_STORAGE.md) and [current acceptance](experiments/canvas-packed-wire.md).
The earlier Shell, screen enum and global dashboard navigation actions are retired.

## Editing Test Playground card order

Test Playground owns Edit/Done and the wrapping demo cards. Its independent
`app:test-playground/state` holds seven position registers. A move changes only
the moved card's register; other connected clients animate the order while keeping
their own editing mode. Landscape uses three columns and portrait uses two.
Keyboard arrows, cancellation and reflow use the existing bounded reorder runtime
from [RFC 0015](rfcs/0015-editable-home-reflow.md).

Home has no Edit mode or writable fields. Its new empty `app:home/state-v4`
binding retires the earlier Home card arrangement; the seven demo identities,
behavior and their existing State references retain their data. Existing
historical experiment reports describe the former Home arrangement.

## Shareable screen URLs

Home opens at `/` and owns `/test-playground` and `/monitoring`. The hub owns
`/test-playground/components`, `/test-playground/layouts`,
`/test-playground/long-document`, `/test-playground/text-panning`,
`/test-playground/text-input`, `/test-playground/animations-two-d` and
`/test-playground/animations-three-d`. Opening or refreshing a path restores that screen;
Back/Forward follows visited paths. Keyboard visibility, focus, Edit mode and drag
state remain local. Monitoring links still require sign-in before private admission.

The server serves a non-cached entry page at each catalog app path. That page pins
its script and stylesheet to one immutable signed release. Release discovery and
compatible app updates retain the screen URL; executable upgrades reload it.
The opening release-control v1 descriptor still carries its immutable `web_entry`
for host compatibility. It is an internal code reference.

Each owning UI names a child navigation edge with
`Destination::app("package-id").route("child-name")`. Ordinary app links omit
`.route(...)` and refer to a destination's stable identity. The packer derives
catalog paths by walking named edges from the first catalog screen; there is no
hand-maintained global route map. Every screen has one route owner; duplicate
sibling names, missing targets, cycles and unreachable screens fail packing.

Native uses the identical resolved path (`--screen test-playground/layouts`, also
accepting a leading `/`). The catalog binds each path to its UI document and
app-owned State reference. Future child screens may explicitly share their
owning app's State; the routing mechanism does not require demonstration screens.
Back first dismisses a local keyboard, then opens the parent path; a top-level
screen returns to the catalog entry screen.

Changing a route name in Home's UI rebases descendant paths without changing their
identities, UI documents, behavior or State. A compatible UI publication updates
the catalog; an open child retains its mount and the browser replaces its current
history entry with the new path. Previously copied old paths return 404; this
milestone does not retain aliases. Legacy hash paths and release-entry redirects
are not supported. Private admission precedes screen payloads.
