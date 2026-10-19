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
