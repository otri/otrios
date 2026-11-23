<!-- SPDX-License-Identifier: Apache-2.0 -->
# Home and reusable input

Home groups installed apps by Space. The public launchers are **The Brass Relay**,
**OIB Designer** and **Test Playground**; Monitoring belongs to Operations.
Demo launchers live under Test Playground, including **Touch Tracing**. Light and
dark themes share navigation, typography and brass/copper icon motifs. Breadcrumbs
use `OTriOS › The Brass Relay`, `OTriOS › Test Playground › Components` and
`OTriOS › Monitoring`; Space names stay out of breadcrumbs. Monitoring retains
its private admission boundary and fixed content aspect below the shared header.
Touch Tracing deliberately uses the whole screen and supplies its own Exit control.

Every Test Playground screen reflows between 1280×800 landscape and 800×1280
portrait. Motion demos own a flexible stage below their controls; the same stage
bounds drive native and browser geometry. Text Input keeps readable key heights
and fits all software keys across portrait width. Monitoring remains landscape,
with its content fitted inside the outlined screen below the shared header.

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

Native evdev collection and dispatch have a dedicated input thread, independent
of the graphics thread's uploads, drawing and page flips. Samples retain their
kernel timing and capture the presented activation/viewport until release;
replacement, revocation or a dropped evdev stream cancels the old contact.
The state owner applies edits locally without waiting for a network receipt.
Held software keys have a brass highlight, with a short release pulse so taps
collected between display frames still get feedback. This uses the effects
layer and retains keyboard geometry and text buffers on contact changes.

Browsers use real HTML inputs/textarea over the authored field bounds. A tap
focuses the element directly in the user gesture, allowing the iPhone system
keyboard and native selection, cut/copy/paste, autocorrect replacement and IME.
Browser fields own their visible text, caret, selection and scrolling; the native
frame retains the shared primitive keyboard and momentum preview behavior above.
The browser face is the same licensed Roboto source, with shared logical metrics,
kerning and ligatures disabled to match the current renderer. The viewport fits
the visible area above a mobile keyboard without flipping the scene orientation
just because the keyboard reduced its height. Pinch zoom remains enabled.
The host supplies the standard layout extent separately when qualifying a new
non-chat app. Returning Home therefore never tries to admit its launcher at the
outgoing 534-unit phone chat width or a keyboard-shortened chat height. The
current chat retains its presented extent until the destination qualifies.

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

## Source-only legacy Playground card order

The source-only legacy Test Playground UI owns Edit/Done and wrapping demo cards.
The managed launcher instead derives its children from signed catalog paths and
Space attachments, so adding a child does not rewrite these legacy registers.
The legacy UI Its independent
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
`/test-playground/animations-three-d`, and
`/test-playground/touch-tracing`. Opening or refreshing a path restores that screen;
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
The host Back action first dismisses a local keyboard, then opens the parent
path; a top-level screen returns to the catalog entry screen. A breadcrumb
directly opens its named ancestor.

Changing a route name in Home's UI rebases descendant paths without changing their
identities, UI documents, behavior or State. A compatible UI publication updates
the catalog; an open child retains its mount and the browser replaces its current
history entry with the new path. Previously copied old paths return 404; this
milestone does not retain aliases. Legacy hash paths and release-entry redirects
are not supported. Private admission precedes screen payloads.

## Spaces panels and decorative input

The common Spaces entrance and its local panels use the same runtime hit testing
on Web and Frame. Add App, app lists, Deleted Spaces, move destinations and the
native Transfers file list scroll through their complete admitted lists. Member
and retained-data lists use scrolling within bounded batches only when their full
admitted sizes would exceed the current layout/control limits: eight member rows
with the native software keyboard, or sixteen retained datasets. The map groups
up to six Spaces and sixteen preview tiles within its existing scene/coordinate
budget. Changing one of these exceptional batches starts at its top and preserves
an unsubmitted access draft. No state or admission limit is increased.

`TouchResponder::PassThrough` paints a scene subtree normally but excludes it
from pointer hit testing, semantic controls and assistive activation. It exposes
siblings beneath it. `Bubble` instead follows the current target's ancestors;
`Consume` blocks delivery behind the cover. Ordinary covers keep their existing
behavior. A captured gesture is cancelled if its owner gains a pass-through
ancestor.

The home fade and local drag preview are pass-through decoration. Scrolling
surfaces retain the visible part of a clipped control's hit rectangle, adjusting
its children to preserve paint positions. A tile entering the fade can therefore
be clicked or tapped before it is fully visible. Completely offscreen controls
remain absent from semantic navigation; settings backdrops remain inert.

Edit/Done floats over the home fade. The map viewport reaches the bottom edge;
its last 80 logical units are scrolling content padding, rather than a permanently
reserved blank toolbar. The fade ends at the screen edge and lets input through
around the fixed controls. At the end of a scroll, the spacer raises the final
content above those controls. Browser map canvases align to the bottom of their
visible window while retaining uniform scaling.

## Local ink and developer preferences

`Presentation::TouchPad` accepts optional `InkOptions`. Without options it keeps
its existing shared five-second ghost trails. With options and no `drawing` binding it uses local ink:
`lifetime_ms: None` retains strokes until Clear or unmount; a finite lifetime
expires them. The segment budget is 1–64, each with at most 192 points. Long
strokes share segment endpoints and evict oldest segments at the bound.
`Operation::Ink` consumes Pad gestures without replicated field writes;
`Operation::ClearInk` clears only that mount. Full-screen presentation hides the
host breadcrumb without introducing app-name-specific runtime behavior. Grid,
border, toolbar and labels remain app-authored UI. See [Touch Tracing](../apps/touch-trace/README.md).

Signed-in profiles expose **Developer Mode**, off by default. Enabling it reveals
**Hit Zone on Touch** and **System / Light / Dark** appearance controls. These
preferences persist in the authenticated self-profile, with revision checks;
they neither change Space roles nor grant access. Ordinary touch feedback stays
hidden unless both Developer Mode and Hit Zone on Touch are enabled. Keyboard
focus indication and responder routing remain independent.


`InkOptions.drawing` opts into Space-owned shared strokes and a device-local
uniform camera. The grid covers the viewport in square world cells. One touch
draws; two touches pan/pinch. Mouse right/middle or Space drag pans; wheel behavior
can be tested with **Scroll: Pan / Zoom** in Developer Mode. Pinch/Ctrl-wheel and
Safari trackpad gestures zoom around their anchor. See [RFC 0039](rfcs/0039-shared-drawing.md).
