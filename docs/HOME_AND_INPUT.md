<!-- SPDX-License-Identifier: Apache-2.0 -->
# Home and reusable input

Home is an admitted Canvas app and launcher. Its navigation targets UI/State
references for Component Playground, Layout Lab, Input Lab, Animation Lab, 3D Lab
and private Monitoring. Home has no Back/Home controls of its own. Other app
headers own their navigation; Input Lab uses Home without a redundant Back.
Monitoring sign-out uses the same component styling and server revocation.

Navigation and route history stay on the device. A candidate Canvas qualifies
UI, behavior and selected state before accepting its lease; a rejected candidate
retains the current activation. The host does not select a compiled screen UI.
Friendly route aliases remain bookmark conveniences over admitted app identities.

Input Lab stores two shared atomic text registers. The reusable primitive keyboard
and field bindings live in the app package. Caret, selection, focus, keyboard page,
repeat timing and previews stay local in RAM. A tap chooses the caret from actual
font advances, grapheme boundaries and the displayed text window. Switching fields
and repeated keys use the same responder/input implementation on both hosts.

The browser also forwards ordinary keyboard/text input and exposes invisible
semantic controls for accessibility. It does not draw a second DOM text field.
Physical touch uses the frame's existing accepted calibration and the same logical
scene coordinates. A code reload retains confirmed text through server state but
restarts local focus and editing presentation. This is register-level shared text,
not a sequence CRDT or full IME/shaping engine.

See [app sources](../apps/README.md), [scene routing](SCENE_TREE.md),
[storage](CLIENT_STORAGE.md) and [current acceptance](experiments/canvas-packed-wire.md).
The earlier Shell, screen enum and global dashboard navigation actions are retired.

## Editing Home card order

Home's UI document declares the card components and a wrapping flow. The shared
Home state holds a position register per card; a move changes that one register.
Portrait and landscape use the same order with two and three columns respectively.

Choose **Edit** at the top right, then drag a card to another slot. Other connected
clients animate the new order while retaining their own interaction mode.
**Done** restores card launching. Keyboard arrows move a focused card while editing.
Editing, wiggle, pointer coordinates and animation frames stay local. Cancelling
a gesture retains moves already sent; rotating cancels capture before reflow.

The reusable runtime behavior and its limits are described in
[RFC 0015](rfcs/0015-editable-home-reflow.md). This first release adds the runtime
support and starts Home's previously empty state at `app:home/state-v3`; the other
app state roots retain their contents. Later compatible arrangement/style edits
can use the existing Home UI-only publisher.

## Shareable screen URLs

Browser navigation uses screen paths such as `/home`, `/component-playground`,
`/layout-lab`, `/input-lab`, `/animation-lab`, `/three-d-lab` and `/monitoring`.
Opening or refreshing a path restores that screen; Back/Forward follows screen
navigation. Keyboard visibility, focus, Edit mode and drag state remain local.
Monitoring links still require sign-in before private app admission.

The server serves a non-cached entry page at each catalog app path. That page pins
its script and stylesheet to one immutable signed release. Release discovery and
compatible app updates retain the screen URL; executable upgrades reload it.
The opening release-control v1 descriptor still carries its immutable `web_entry`
for host compatibility. It is an internal code reference.

Old hash links normalize to the screen path. Browser navigation to an old release
entry redirects to the current entry, including expired release bookmarks; direct
asset fetches continue returning the exact immutable bytes. Unknown screens and
private/API asset paths retain their normal HTTP boundaries.
