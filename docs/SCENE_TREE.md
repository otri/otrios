# Primitive scene tree and responder chain

The dashboard now expands its immutable application view into a validated, immutable scene tree. The root is a 1280×800 `Canvas`. Native GLES2 and browser WebGL draw the same primitive geometry; the browser has no separate visual panel/switch implementation. Its invisible semantic buttons provide keyboard and assistive access.

## Primitives and components

| Primitive | Responsibility |
| --- | --- |
| Canvas | Parent-local child frames, ordered children, optional rectangular clipping |
| Viewport | Uniformly scaled child coordinates with rectangular clipping; painting, hit testing and semantic bounds share the transform |
| Text | Bounded text, admitted vector-font metrics and cached glyph coverage |
| Superellipse | Filled rectangle with superelliptic corners, radius and exponent; radius zero gives a rectangle |
| Circle | Filled circle within a square frame |
| Path | Bounded stroked polyline for graph data |

Superellipse exponent 2 gives circular corners; the components use exponent 4. This is a rounded rectangle with superelliptic corners, not a claim that its entire outline follows one full-width Lamé oval. Painting tessellates curves into GPU triangles; hit testing evaluates their analytic boundaries, so subpixel tessellation differences remain possible at an edge.

Panel and Switch are now FlexBuffers component templates interpreted at runtime. Panel expands into a Canvas, Superellipse background and child slot. Switch expands into a panel, Text, a nested control Canvas, a Superellipse track and Circle knob, with a touch responder and switch accessibility semantics on its outer Canvas. The graph is a Panel containing Text, grid rectangles, a Path and a Circle. None is a compositor primitive or a special case in the hit tester.

```text
Canvas (screen)
├─ Superellipse (screen background, radius 0)
├─ Text (heading)
├─ Canvas (focus switch component; activating responder)
│  ├─ Superellipse (panel background)
│  ├─ Text (label and state)
│  ├─ Canvas (switch track)
│  │  ├─ Superellipse
│  │  └─ Circle
│  └─ Text (hint)
├─ Canvas (graph panel and primitive children)
└─ Canvas (other labels/panels)
```

Component templates and instances live in each app's admitted FlexBuffers UI.
[BINARY_LAYOUT.md](BINARY_LAYOUT.md) describes explicit selector slots, scoped
responders and the package workflow. Canvas resolves immutable selected values
into scenes; no global DashboardView or embedded default dashboard participates.

## Pointer delivery

1. The host maps a press into logical root coordinates and supplies its pointer ID and phase.
2. Hit testing walks children in reverse paint order. Parent-local transforms and every enabled Canvas clip apply. Actual circle/superellipse shape boundaries participate; a Canvas is a container, so its empty areas do not automatically intercept touches.
3. The deepest visible node becomes the target. Delivery walks that node, its parent and successive ancestors. Each visit contains coordinates local to that node. A missing responder or `Bubble` passes to the parent; `Consume` stops; `Activate(action)` stops and captures the pointer.
4. Move, release and cancel follow the captured target's chain. A drag cannot retarget another sibling. `Activate` emits its action exactly once on release within the captured responder's visible subtree. A press outside followed by release inside does not activate it. Release outside or cancellation emits no action.
5. Capture pins the original immutable scene for geometry during the gesture. If a new scene removes or rebinds its responder ID, the pending gesture is cancelled. Otherwise a concurrent CRDT update may publish a new scene without losing the gesture; its eventual application action operates on current app state.

The tree is not traversed again through obscured siblings after a target declines an event: bubbling follows ancestry. This is the initial responder chain, with no separate root-to-target preview/capture phase or gesture-recognizer arbitration. Pointer capture refers to delivery ownership, not a preview event phase.

The router bounds active pointers to 16 and validates finite input. Pointer capture and focus/pressed interaction state are local, not shared CRDT data. The native SDL adapter forwards calibrated touch-generated mouse down/motion/up as one pointer, captures the mouse during handled gestures and cancels on focus loss. The browser forwards Pointer Events, uses browser pointer capture, and cancels on cancellation, lost capture and blur. The core can maintain multiple browser pointer IDs independently. Keyboard/assistive activation and native Space follow the same node-to-parent chain; the browser no longer toggles app state directly.

## Validation and limits

Scene construction rejects duplicate identities, nonfinite/oversized geometry, invalid circle/superellipse/path fields, children on primitives other than Canvas/Viewport, more than 512 nodes and depth beyond 16. Only a clipped root at the fixed logical extent is accepted. Component IDs reserve stable descendant ranges across checked/unchecked states and graph sample changes. Clipping trims actual triangles, including text and curves, rather than just skipping wholly outside shapes.

Runtime tests cover nested local-coordinate bubbling, consume/stop behavior and overlapping siblings, independent pointer captures, duplicate release, cancellation and responder removal, drag-out release, concurrent state changes during a gesture, geometric hit tests, clipped triangles and malformed trees. Native/browser host builds remain part of CI. The tiny demo font, absolute frames, translation plus uniform Viewport scaling, rectangular Canvas clipping and explicit bounded application/navigation/input actions are intentional first-slice limits. General layout, arbitrary affine transforms, vector fonts, accessibility-tree generalization and gesture recognition are not implemented here.

The [local shell and core keyboard](HOME_AND_INPUT.md) reuse this chain. Navigation cancels captures. Keyboard contacts use the same painted key ancestry, with an explicit keyboard model for slide-to-neighbour and bounded editing-key repeat instead of ordinary button activation on the originally pressed key.
